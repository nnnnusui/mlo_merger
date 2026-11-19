use super::*;
use crate::common::changes::StoreChanges;
use rayon::prelude::*;

impl IncrementalMerge<'_> {
  /// Rebuilds dependency groups while retaining unchanged outputs and negative results; returns
  /// the merged files the staged output adds, changes or removes, or `None` when the output is
  /// current and nothing was staged.
  pub(in crate::app::map::merge) fn run(&self) -> Result<Option<StoreChanges>> {
    let previous = MergeMetadata::load(self.output)?;
    let mut previous_inputs = BTreeMap::new();
    if let Some(metadata) = &previous {
      for record in metadata.files.values() {
        for input in record.merge_sources.iter().chain(record.vanilla.iter()) {
          previous_inputs.insert(input.path.clone(), &input.fingerprint);
        }
      }
    }
    let baselines = self
      .ybn
      .iter()
      .chain(self.ymap)
      .map(|path| Ok((file_name(path)?, path)))
      .collect::<Result<BTreeMap<_, _>>>()?;
    let mut sources = BTreeMap::<String, Vec<&MergeSourceFile>>::new();
    for source in self.inputs.ybn.iter().chain(&self.inputs.ymap) {
      sources.entry(source.file_name.to_ascii_lowercase()).or_default().push(source);
    }
    // Carried-over changes are merged like a source into the vanilla file they change.
    for source in self.carried {
      let name = source.file_name.to_ascii_lowercase();
      if baselines.contains_key(&name) {
        sources.entry(name).or_default().push(source);
      } else {
        tracing::warn!(
          "{} changes {name}, which is not a latest vanilla YMAP; not applied",
          source.stored_path
        );
      }
    }
    // A vanilla overwrite is merged like a source, even into a file no resource replaces.
    for name in self.vanilla_overwrites.keys() {
      if self.vanilla_overwrite(name).is_some() && baselines.contains_key(name) {
        sources.entry(name.clone()).or_default();
      } else {
        tracing::debug!("Vanilla overwrite of {name} names no latest vanilla YMAP; not applied");
      }
    }
    // A YMAP of a superseded generation is not merged: the source cache carries its changes to the
    // latest generation, its sources are omitted and the vanilla file stays.
    let mut superseded = BTreeMap::<String, (String, Vec<&MergeSourceFile>)>::new();
    if let Some(generations) =
      crate::app::map::vanilla_cache::generations::GenerationIndex::load(self.vanilla_cache)?
    {
      let names =
        sources.keys().filter(|name| name.ends_with(".ymap")).cloned().collect::<Vec<_>>();
      for name in names {
        let Some(successor) = generations.successor(&name) else {
          continue;
        };
        if self.vanilla_overwrite(&name).is_some() {
          tracing::warn!(
            "The vanilla overwrite of {name} is not applied, as {successor} supersedes it"
          );
        }
        let files = sources.remove(&name).unwrap_or_default();
        superseded.insert(name, (successor, files));
      }
    }
    let graph = self.ymap_graph()?;
    let mut groups = groups(&sources, &graph);
    // A superseded YMAP is recorded on its own below, whatever its relations.
    for group in &mut groups {
      group.names.retain(|name| !superseded.contains_key(name));
    }
    groups.retain(|group| !group.names.is_empty());
    let now = jiff::Timestamp::now().to_string();
    let context = GroupContext {
      sources: &sources,
      baselines: &baselines,
      graph: &graph,
      previous: previous.as_ref(),
      previous_inputs: &previous_inputs,
      now: &now,
    };
    // Groups share no files and no parent references, so they are merged in parallel, those
    // with the most input bytes first, as they take longest; worker threads are handed the
    // active matching tolerances.
    let tolerances = crate::core::config::matching::current();
    let input_bytes = |group: &Group| {
      let paths = group.names.iter().flat_map(|name| {
        let sources = sources.get(name).into_iter().flatten().map(|source| &source.path);
        baselines.get(name).copied().into_iter().chain(sources)
      });
      paths.map(|path| fs::metadata(path).map_or(0, |metadata| metadata.len())).sum::<u64>()
    };
    let mut order = groups.iter().map(input_bytes).enumerate().collect::<Vec<_>>();
    order.sort_by_key(|&(index, bytes)| (std::cmp::Reverse(bytes), index));
    let order = order.into_iter().map(|(index, _)| index).collect::<Vec<_>>();
    let mut outcomes = order
      .into_par_iter()
      .map(|index| {
        let group = &groups[index];
        let first = group.names.first().map_or("", String::as_str);
        crate::common::log_block::in_one_block(tracing::info_span!("group", first), || {
          crate::core::config::matching::with_tolerances(tolerances, || {
            self.merge_or_reuse(index, group, &context)
          })
        })
        .map(|outcome| (index, outcome))
        .map_err(|error| format!("Merge group of {first}: {error}"))
      })
      .collect::<std::result::Result<Vec<_>, String>>()?;
    outcomes.sort_by_key(|(index, _)| *index);
    let mut files = BTreeMap::new();
    let mut omitted = BTreeSet::new();
    let (mut rebuilt, mut reused) = (0, 0);
    let mut retained_files = Vec::new();
    for (_, outcome) in outcomes {
      if outcome.rebuilt {
        rebuilt += 1;
      } else {
        reused += 1;
      }
      retained_files.extend(outcome.retained);
      for record in outcome.planned.values().filter(|record| record.output.is_some()) {
        for input in &record.merge_sources {
          if let Some(original) = &input.original_path {
            omitted.insert(original.to_string_lossy().replace('\\', "/"));
          }
        }
      }
      files.extend(outcome.planned);
    }
    for (name, (successor, sources)) in superseded {
      let merge_sources = sources
        .iter()
        .map(|source| self.source_input(source, &previous_inputs))
        .collect::<Result<Vec<_>>>()?;
      for input in &merge_sources {
        if let Some(original) = &input.original_path {
          omitted.insert(original.to_string_lossy().replace('\\', "/"));
        }
      }
      let merged_at = previous
        .as_ref()
        .and_then(|previous| previous.files.get(&name))
        .filter(|old| {
          old.merge_sources == merge_sources && old.superseded_by.as_ref() == Some(&successor)
        })
        .map_or_else(|| now.clone(), |old| old.merged_at.clone());
      files.insert(
        name,
        FileRecord {
          merge_sources,
          vanilla: None,
          dependencies: BTreeSet::new(),
          replaces_other_resources: false,
          dependency_fingerprint: String::new(),
          output: None,
          superseded_by: Some(successor),
          merged_at,
          duplicates: Vec::new(),
        },
      );
    }
    let omit_text = omitted.into_iter().collect::<Vec<_>>().join("\n");
    let report = self.report(&files, &baselines)?;
    let report_current = super::super::report::is_current(self.output, &report)?;
    if rebuilt == 0
      && !self.force
      && previous.as_ref().is_some_and(|previous| previous.files == files)
      && previous.as_ref().is_some_and(|previous| {
        previous.algorithm_version.is_none() && previous.algorithm_versions == algorithm_versions()
      })
      && report_current
      && fs::read_to_string(self.output.join("_omit.txt")).ok().as_deref() == Some(&omit_text)
    {
      tracing::info!("Merge output is current; nothing to do");
      return Ok(None);
    }
    let changes = output_changes(previous.as_ref(), &files);
    for (from, to) in retained_files {
      retain_file(&from, &to)?;
    }
    fs::write(self.staging.join("_omit.txt"), omit_text)?;
    for (path, bytes) in &report {
      let path = output_path(self.staging, path)?;
      fs::create_dir_all(path.parent().ok_or("Report path has no parent")?)?;
      fs::write(path, bytes)?;
    }
    write_json(
      &self.staging.join(crate::app::map::merge::outputs::CACHE_INFO),
      &MergeMetadata {
        format_version: 1,
        algorithm_version: None,
        algorithm_versions: algorithm_versions(),
        generated_at: now,
        files,
      },
    )?;
    tracing::info!("Incremental merge: {rebuilt} groups rebuilt, {reused} groups reused");
    Ok(Some(changes))
  }

  /// Reuses `group`'s previous outputs when its inputs are unchanged, or else merges it into
  /// the staging directory; `index` keeps its working files apart from other groups'.
  fn merge_or_reuse(
    &self,
    index: usize,
    group: &Group,
    context: &GroupContext<'_>,
  ) -> Result<GroupOutcome> {
    let mut retained = Vec::new();
    let mut planned = BTreeMap::new();
    for name in &group.names {
      let merge_sources = context
        .sources
        .get(name)
        .into_iter()
        .flatten()
        .map(|source| self.source_input(source, context.previous_inputs))
        .collect::<Result<Vec<_>>>()?;
      let vanilla = context
        .baselines
        .get(name)
        .map(|path| {
          let stored = self.stored_vanilla_path(path);
          Ok::<_, Box<dyn std::error::Error>>(InputFile {
            fingerprint: Fingerprint::read(
              path,
              context.previous_inputs.get(&stored).copied(),
              self.force,
            )?,
            path: stored,
            resource: None,
            original_path: None,
            overwrite: self.vanilla_overwrite(name).map(VanillaOverwrite::digest),
            implied: None,
          })
        })
        .transpose()?;
      planned.insert(
        name.clone(),
        FileRecord {
          merge_sources,
          vanilla,
          dependencies: context.graph.get(name).cloned().unwrap_or_default(),
          replaces_other_resources: self.added.get(name).is_some_and(|file| file.replaces_others()),
          dependency_fingerprint: String::new(),
          output: None,
          superseded_by: None,
          merged_at: context.now.to_string(),
          duplicates: Vec::new(),
        },
      );
    }
    let signature = signature(group.extension, &planned)?;
    let mut current = !self.force
      && context
        .previous
        .as_ref()
        .map(|metadata| metadata.version_matches(group.extension))
        .transpose()?
        .unwrap_or(false);
    for name in &group.names {
      let Some(old) = context.previous.and_then(|metadata| metadata.files.get(name)) else {
        current = false;
        break;
      };
      if let Some(output) = &old.output {
        let path = Path::new(&output.path);
        if path.parent() != Some(Path::new(group.extension)) || file_name(path)? != *name {
          current = false;
          break;
        }
      } else if fs::exists(self.output.join(group.extension).join(name)).unwrap_or(false) {
        current = false;
        break;
      }
      if !reusable(old, &signature, self.output, false)? {
        current = false;
        break;
      }
    }
    let rebuilt = !current;
    if current {
      for (name, record) in &mut planned {
        let old = &context.previous.ok_or("Merge cache missing")?.files[name];
        record.merged_at = old.merged_at.clone();
        record.duplicates = old.duplicates.clone();
        if let Some(output) = &old.output {
          let from = output_path(self.output, &output.path)?;
          let to = output_path(self.staging, &output.path)?;
          retained.push((from.clone(), to));
          record.output = Some(OutputFile {
            path: output.path.clone(),
            fingerprint: Fingerprint::read(&from, Some(&output.fingerprint), false)?,
          });
        }
      }
      tracing::debug!("Reusing {} merge group ({} files)", group.extension, group.names.len());
    } else {
      tracing::info!("Updating {} merge group ({} files)", group.extension, group.names.len());
      let (duplicates, outputs) =
        self.merge_group(index, group, context.sources, context.baselines)?;
      for (name, entries) in duplicates.files {
        if let Some(record) = planned.get_mut(&name) {
          record.duplicates = entries;
        }
      }
      for path in outputs {
        if let Some(record) = planned.get_mut(&file_name(&path)?) {
          record.output = Some(OutputFile {
            path: path.strip_prefix(self.staging)?.to_string_lossy().replace('\\', "/"),
            fingerprint: Fingerprint::read(&path, None, true)?,
          });
        }
      }
    }
    for record in planned.values_mut() {
      record.dependency_fingerprint = signature.clone();
    }
    Ok(GroupOutcome {
      planned,
      retained,
      rebuilt,
    })
  }
}

impl IncrementalMerge<'_> {
  /// The merge report of `files`, the records of this merge, and of the changes carried nowhere;
  /// `baselines` are the latest vanilla files by name.
  fn report(
    &self,
    files: &BTreeMap<String, FileRecord>,
    baselines: &BTreeMap<String, &PathBuf>,
  ) -> Result<BTreeMap<String, Vec<u8>>> {
    use super::super::report::{ReportInput, ReportedOutput, build};
    let stream_paths = self
      .inputs
      .ymap
      .iter()
      .map(|source| (source.stored_path.replace('\\', "/"), source.stream_path.clone()))
      .collect::<BTreeMap<_, _>>();
    let mut left_behind = BTreeMap::<String, Vec<_>>::new();
    for change in self.left_behind {
      left_behind.entry(change.file_name.to_ascii_lowercase()).or_default().push(change.clone());
    }
    let names = files.keys().cloned().chain(left_behind.keys().cloned()).collect::<BTreeSet<_>>();
    let vanilla = names
      .iter()
      .map(|name| {
        let path = baselines
          .get(name)
          .map(|path| self.stored_vanilla_path(path).to_string_lossy().replace('\\', "/"));
        (name.clone(), path)
      })
      .collect::<BTreeMap<_, _>>();
    let inputs = names
      .iter()
      .map(|name| {
        let record = files.get(name);
        ReportInput {
          file_name: name,
          vanilla: vanilla[name].as_deref(),
          output: match record {
            Some(record) if record.superseded_by.is_some() => ReportedOutput::Omitted,
            Some(record) if record.output.is_some() => ReportedOutput::Written,
            _ => ReportedOutput::Unchanged,
          },
          superseded_by: record.and_then(|record| record.superseded_by.as_deref()),
          duplicates: record.map(|record| record.duplicates.as_slice()).unwrap_or_default(),
          left_behind: left_behind.remove(name).unwrap_or_default(),
        }
      })
      .collect();
    build(inputs, &stream_paths)
  }

  /// The record of `source` as a merge input, its fingerprint reused from `previous_inputs` when
  /// the file is unchanged.
  fn source_input(
    &self,
    source: &MergeSourceFile,
    previous_inputs: &BTreeMap<PathBuf, &Fingerprint>,
  ) -> Result<InputFile> {
    let stored = PathBuf::from(&source.stored_path);
    let previous = previous_inputs.get(&stored).copied();
    let mut fingerprint = Fingerprint::read(&source.path, previous, self.force)?;
    // Carried-over changes are written again on every run; the same content is the same input.
    if let Some(previous) = previous.filter(|previous| previous.same_content(&fingerprint)) {
      fingerprint = previous.clone();
    }
    let implied = source
      .implied_path
      .as_ref()
      .map(|path| Ok::<_, Box<dyn std::error::Error>>(hex::encode(Sha256::digest(fs::read(path)?))))
      .transpose()?;
    Ok(InputFile {
      fingerprint,
      path: stored,
      resource: Some(source.resource.clone()),
      // Carried-over changes replace no source file.
      original_path: (!source.propagated).then(|| PathBuf::from(&source.source_path)),
      overwrite: self.overwrite(source).map(StreamFileOverwrite::digest),
      implied,
    })
  }
}

/// The merged files `files` records that differ from those of the `previous` merge.
fn output_changes(
  previous: Option<&MergeMetadata>,
  files: &BTreeMap<String, FileRecord>,
) -> StoreChanges {
  let outputs = |files: &BTreeMap<String, FileRecord>| {
    files
      .values()
      .filter_map(|record| record.output.as_ref())
      .map(|output| (output.path.clone(), output.fingerprint.sha256.clone()))
      .collect::<BTreeMap<_, _>>()
  };
  let mut before = previous.map(|metadata| outputs(&metadata.files)).unwrap_or_default();
  let mut changes = StoreChanges::default();
  for (path, sha256) in outputs(files) {
    let previous = before.remove(&path);
    changes.record(path, previous.as_deref(), Some(&sha256));
  }
  for (path, sha256) in before {
    changes.record(path, Some(&sha256), None);
  }
  changes
}

/// What every group's merge reads.
struct GroupContext<'a> {
  sources: &'a BTreeMap<String, Vec<&'a MergeSourceFile>>,
  baselines: &'a BTreeMap<String, &'a PathBuf>,
  graph: &'a BTreeMap<String, BTreeSet<String>>,
  previous: Option<&'a MergeMetadata>,
  previous_inputs: &'a BTreeMap<PathBuf, &'a Fingerprint>,
  now: &'a str,
}

/// One group's records, the previous outputs it keeps and whether it was merged again.
struct GroupOutcome {
  planned: BTreeMap<String, FileRecord>,
  retained: Vec<(PathBuf, PathBuf)>,
  rebuilt: bool,
}
