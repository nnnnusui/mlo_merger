use super::*;
use crate::app::map::merge::ybn_conflicts::YbnSource;
use std::collections::HashMap;

impl IncrementalMerge<'_> {
  /// Merges `group` in its own working directory below the staging directory, named by its
  /// `index`, then moves the merged files into the staging directory's `ybn/` or `ymap/`;
  /// returns the group's duplicate report, the moved files and the repeats left out of each
  /// YBN.
  pub(super) fn merge_group(
    &self,
    index: usize,
    group: &Group,
    sources: &BTreeMap<String, Vec<&MergeSourceFile>>,
    baselines: &BTreeMap<String, &PathBuf>,
  ) -> Result<(DuplicateReport, Vec<PathBuf>, BTreeMap<String, usize>)> {
    let mut vanilla = group
      .names
      .iter()
      .filter_map(|name| baselines.get(name).map(|path| (*path).clone()))
      .collect::<Vec<_>>();
    let sources = group
      .names
      .iter()
      .flat_map(|name| sources.get(name).into_iter().flatten().copied())
      .collect::<Vec<_>>();
    let mut duplicates = DuplicateReport {
      format_version: 1,
      files: BTreeMap::new(),
    };
    let work = self.staging.join(format!(".group-{index}"));
    // A YMAP resources add is merged against an empty YMAP.
    let plans =
      group.names.iter().filter_map(|name| Some((name, self.added.get(name)?))).collect::<Vec<_>>();
    for (name, _) in &plans {
      let path = work.join("added").join(name);
      fs::create_dir_all(path.parent().ok_or("Baseline path has no parent")?)?;
      fs::write(&path, crate::app::map::source_cache::empty_baseline_bytes()?)?;
      vanilla.push(path);
    }
    let mut repeats = BTreeMap::new();
    let merged = if group.extension == "ybn" {
      MergeYbnConflicts {
        source_dir: self.source_cache.to_path_buf(),
        output_dir: work.clone(),
        omitted_files_path: work.join(".ybn_omit.txt"),
        // A YBN of the group no source changes is in it to leave its repeats out.
        standalone: group
          .names
          .iter()
          .filter(|name| !sources.iter().any(|source| source.file_name.eq_ignore_ascii_case(name)))
          .cloned()
          .collect(),
      }
      .run_sources(
        &vanilla,
        &sources
          .iter()
          .map(|source| YbnSource {
            file_name: source.file_name.clone(),
            path: source.path.clone(),
            diff_path: source.diff_path.clone(),
            overwrite: self.overwrite(source).cloned(),
            propagated: source.propagated,
          })
          .collect::<Vec<_>>(),
      )?;
      let recorded = work.join(crate::app::map::merge::ybn_conflicts::REPEATS_FILE);
      if fs::is_file(&recorded) {
        repeats = crate::common::json::read_json(&recorded)?;
      }
      work.clone()
    } else {
      let vanilla_overwrites = group
        .names
        .iter()
        .filter_map(|name| Some((name.clone(), self.vanilla_overwrite(name)?.clone())))
        .collect::<HashMap<_, _>>();
      if sources.is_empty() && vanilla_overwrites.is_empty() {
        return Ok((duplicates, Vec::new(), repeats));
      }
      let directory = work.join("ymap");
      let (propagated, stream_files): (Vec<&MergeSourceFile>, Vec<&MergeSourceFile>) =
        sources.iter().copied().partition(|source| source.propagated);
      // The versions of a YMAP resources add are merged in the order the overwrite cache
      // records, the one loaded last first.
      let mut stream_files = stream_files;
      stream_files.sort_by_key(|source| {
        self
          .added
          .get(&source.file_name.to_ascii_lowercase())
          .and_then(|file| file.merge_position(&source.resource))
          .unwrap_or(0)
      });
      let ymap_sources = stream_files
        .iter()
        .map(|source| (source.resource.clone(), source.path.clone(), source.diff_path.clone()))
        .collect::<Vec<_>>();
      let overwrites = stream_files
        .iter()
        .filter_map(|source| Some((source.path.clone(), self.overwrite(source)?.clone())))
        .collect();
      let implied = stream_files
        .iter()
        .filter_map(|source| Some((source.path.clone(), source.implied_path.clone()?)))
        .collect();
      MergeYmap {
        vanilla_dir: self.vanilla_archive.to_path_buf(),
        output_dir: directory.clone(),
        rebuild_all: true,
        overwrites,
        implied,
        vanilla_overwrites,
        propagated: propagated
          .iter()
          .map(|source| (source.resource.clone(), source.file_name.clone(), source.path.clone()))
          .collect(),
        added: plans.iter().map(|(name, _)| (*name).clone()).collect(),
        replacing: plans
          .iter()
          .filter(|(_, file)| file.replaces_others())
          .map(|(name, _)| (*name).clone())
          .collect(),
      }
      .run_with_latest_vanilla_files(&vanilla, &ymap_sources)?;
      duplicates = crate::common::json::read_json(directory.join(".duplicates.json"))?;
      // The report names each file relative to its store, as the merge metadata does.
      let stored = vanilla
        .iter()
        .map(|path| (path.clone(), self.stored_vanilla_path(path)))
        .chain(
          sources.iter().map(|source| (source.path.clone(), PathBuf::from(&source.stored_path))),
        )
        .collect::<HashMap<_, _>>();
      for duplicate in duplicates.files.values_mut().flatten() {
        let contributions = std::iter::once(&mut duplicate.applied)
          .chain(duplicate.ignored.iter_mut().map(|ignored| &mut ignored.source));
        for contribution in contributions {
          if let Some(path) = stored.get(&contribution.path) {
            contribution.path = path.clone();
          }
        }
      }
      directory
    };
    // Only the merged stream files are kept; the merge's own records stay behind.
    let target = self.staging.join(group.extension);
    let mut outputs = Vec::new();
    if fs::is_dir(&merged) {
      for entry in fs::read_dir(&merged)? {
        let path = entry?.path();
        let is_output = fs::is_file(&path)
          && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case(group.extension));
        if is_output {
          fs::create_dir_all(&target)?;
          let output = target.join(path.file_name().ok_or("Merged file name missing")?);
          fs::rename(&path, &output)?;
          outputs.push(output);
        }
      }
    }
    if fs::exists(&work).unwrap_or(false) {
      fs::remove_dir_all(&work)?;
    }
    Ok((duplicates, outputs, repeats))
  }
}
