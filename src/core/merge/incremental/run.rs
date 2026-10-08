use super::*;

impl IncrementalMerge<'_> {
  /// Rebuilds dependency groups while retaining unchanged outputs and negative results.
  pub(in crate::core::merge) fn run(&self) -> Result<bool> {
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
    let graph = self.ymap_graph()?;
    let groups = groups(&sources, &graph);
    let mut files = BTreeMap::new();
    let mut omitted = BTreeSet::new();
    let now = chrono::Utc::now().to_rfc3339();
    let mut rebuilt = 0;
    let mut reused = 0;
    let mut retained_files = Vec::new();
    for group in groups {
      let mut planned = BTreeMap::new();
      for name in &group.names {
        let merge_sources = sources
          .get(name)
          .into_iter()
          .flatten()
          .map(|source| {
            Ok(InputFile {
              path: source.path.clone(),
              resource: Some(source.resource.clone()),
              original_path: Some(source.original_path.clone()),
              fingerprint: Fingerprint::read(
                &source.path,
                previous_inputs.get(&source.path).copied(),
                self.force,
              )?,
            })
          })
          .collect::<Result<Vec<_>>>()?;
        let vanilla = baselines
          .get(name)
          .map(|path| {
            Ok::<_, Box<dyn std::error::Error>>(InputFile {
              path: (*path).clone(),
              resource: None,
              original_path: None,
              fingerprint: Fingerprint::read(
                path,
                previous_inputs.get(*path).copied(),
                self.force,
              )?,
            })
          })
          .transpose()?;
        planned.insert(
          name.clone(),
          FileRecord {
            merge_sources,
            vanilla,
            dependencies: graph.get(name).cloned().unwrap_or_default(),
            dependency_fingerprint: String::new(),
            output: None,
            merged_at: now.clone(),
            duplicates: Vec::new(),
          },
        );
      }
      let signature = signature(&planned)?;
      let mut current = !self.force;
      for name in &group.names {
        let Some(old) = previous.as_ref().and_then(|metadata| metadata.files.get(name)) else {
          current = false;
          break;
        };
        if let Some(output) = &old.output {
          let path = Path::new(&output.path);
          if path.parent() != Some(Path::new(group.extension)) || file_name(path)? != *name {
            current = false;
            break;
          }
        } else if self.output.join(group.extension).join(name).exists() {
          current = false;
          break;
        }
        if !reusable(old, &signature, self.output, false)? {
          current = false;
          break;
        }
      }
      if current {
        reused += 1;
        for (name, record) in &mut planned {
          let old = &previous.as_ref().ok_or("Merge cache missing")?.files[name];
          record.merged_at = old.merged_at.clone();
          record.duplicates = old.duplicates.clone();
          if let Some(output) = &old.output {
            let from = output_path(self.output, &output.path)?;
            let to = output_path(self.staging, &output.path)?;
            retained_files.push((from.clone(), to));
            record.output = Some(OutputFile {
              path: output.path.clone(),
              fingerprint: Fingerprint::read(&from, Some(&output.fingerprint), false)?,
            });
          }
        }
        log::info!("Reusing {} merge group ({} files)", group.extension, group.names.len());
      } else {
        rebuilt += 1;
        log::info!("Updating {} merge group ({} files)", group.extension, group.names.len());
        let duplicates = self.merge_group(&group, &sources, &baselines)?;
        for (name, entries) in duplicates.files {
          if let Some(record) = planned.get_mut(&name) {
            record.duplicates = entries;
          }
        }
        let directory = self.staging.join(group.extension);
        if directory.is_dir() {
          for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            if !path.is_file() {
              continue;
            }
            if let Some(record) = planned.get_mut(&file_name(&path)?) {
              record.output = Some(OutputFile {
                path: path.strip_prefix(self.staging)?.to_string_lossy().replace('\\', "/"),
                fingerprint: Fingerprint::read(&path, None, true)?,
              });
            }
          }
        }
      }
      for record in planned.values_mut() {
        record.dependency_fingerprint = signature.clone();
        if record.output.is_some() {
          for input in &record.merge_sources {
            if let Some(original) = &input.original_path {
              omitted.insert(
                original.strip_prefix(self.source_dir)?.to_string_lossy().replace('\\', "/"),
              );
            }
          }
        }
      }
      files.extend(planned);
    }
    let omit_text = omitted.into_iter().collect::<Vec<_>>().join("\n");
    let duplicates = DuplicateReport {
      format_version: 1,
      files: files
        .iter()
        .filter(|(_, record)| !record.duplicates.is_empty())
        .map(|(name, record)| (name.clone(), record.duplicates.clone()))
        .collect(),
    };
    let duplicate_path = output_path(self.output, "duplicates.json")?;
    let duplicate_current = fs::File::open(&duplicate_path)
      .ok()
      .and_then(|file| serde_json::from_reader::<_, DuplicateReport>(BufReader::new(file)).ok())
      .is_some_and(|previous| previous == duplicates);
    if rebuilt == 0
      && !self.force
      && previous.as_ref().is_some_and(|previous| previous.files == files)
      && duplicate_current
      && fs::read_to_string(self.output.join("_omit.txt")).ok().as_deref() == Some(&omit_text)
    {
      log::info!("Merge output is current; nothing to do");
      return Ok(false);
    }
    for (from, to) in retained_files {
      retain_file(&from, &to)?;
    }
    fs::write(self.staging.join("_omit.txt"), omit_text)?;
    write_json(&self.staging.join("duplicates.json"), &duplicates)?;
    write_json(
      &self.staging.join("merge_cache_info.json"),
      &MergeMetadata {
        format_version: 1,
        algorithm_version: ALGORITHM_VERSION,
        generated_at: now,
        files,
      },
    )?;
    log::info!("Incremental merge: {rebuilt} groups rebuilt, {reused} groups reused");
    Ok(true)
  }
}
