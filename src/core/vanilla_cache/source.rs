use super::*;

pub(super) fn resolve_through_version(
  manifest: &VanillaCacheManifest,
  gamebuild: Option<&GameBuild>,
) -> Result<usize> {
  if manifest.versions.is_empty() {
    return Err("Vanilla archive has no stages".into());
  }
  let Some(gamebuild) = gamebuild else {
    return Ok(manifest.versions.len() - 1);
  };
  let labels = manifest
    .versions
    .iter()
    .map(|version| version.id.split_once('-').map_or(version.id.as_str(), |(_, label)| label))
    .collect::<Vec<_>>();
  gamebuild.last_stage(&labels)
}

pub(super) fn resolve_files(
  manifest: &VanillaCacheManifest,
  through_index: usize,
) -> Result<BTreeMap<String, (String, CachedFile)>> {
  let mut files: BTreeMap<String, (String, CachedFile)> = BTreeMap::new();
  let mut previous_version: Option<&str> = None;
  let mut seen_versions = std::collections::HashSet::new();
  for version in manifest.versions.iter().take(through_index + 1) {
    if version.parent.as_deref() != previous_version || !seen_versions.insert(&version.id) {
      return Err(format!("Invalid vanilla stage chain at {}", version.id).into());
    }
    previous_version = Some(&version.id);
    let mut names_in_stage = std::collections::HashSet::new();
    for (name, change) in &version.changes {
      let normalized = name.to_ascii_lowercase();
      if !names_in_stage.insert(normalized.clone()) {
        return Err(format!("Duplicate vanilla filename {name} at {}", version.id).into());
      }
      match files.get(&normalized) {
        Some((_, previous))
          if change.previous_sha256.as_deref() != Some(previous.sha256.as_str()) =>
        {
          return Err(format!("Invalid vanilla content chain for {name} at {}", version.id).into());
        }
        None if change.previous_sha256.is_some() => {
          return Err(format!("Unexpected previous hash for {name} at {}", version.id).into());
        }
        _ => {}
      }
      files.insert(normalized, (version.id.clone(), change.file.clone()));
    }
  }
  Ok(files)
}

pub(super) fn source_timestamps(
  vanilla_dir: &Path,
  current: &BTreeMap<String, (String, CachedFile)>,
) -> Result<BTreeMap<String, SourceTimestamp>> {
  let mut paths = BTreeSet::from(["cache_info.json".to_owned()]);
  // YMAP version diffs are decoded with its names.
  if vanilla_dir.join("hash_names.json").is_file() {
    paths.insert("hash_names.json".to_owned());
  }
  for (name, (_, file)) in current {
    if is_supported_stream_file(name) {
      paths.insert(file.object.clone());
    }
  }
  paths
    .into_iter()
    .map(|relative| {
      let path = checked_source_path(vanilla_dir, &relative)?;
      let metadata = fs::metadata(path)?;
      let modified = metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?;
      Ok((
        relative,
        SourceTimestamp {
          modified_seconds: modified.as_secs(),
          modified_nanos: modified.subsec_nanos(),
          size: metadata.len(),
        },
      ))
    })
    .collect()
}

pub(super) fn is_supported_stream_file(name: &str) -> bool {
  let extension =
    Path::new(name).extension().and_then(|extension| extension.to_str()).unwrap_or_default();
  extension.eq_ignore_ascii_case("ymap") || extension.eq_ignore_ascii_case("ybn")
}

pub(super) fn existing_artifact_path(
  vanilla_dir: &Path,
  file: &CachedFile,
) -> Result<PathBuf> {
  let path = checked_source_path(vanilla_dir, &file.object)?;
  if !path.is_file() {
    return Err(format!("Vanilla artifact is missing: {}", path.display()).into());
  }
  Ok(path)
}

pub(super) fn checked_source_path(
  vanilla_dir: &Path,
  object: &str,
) -> Result<PathBuf> {
  let relative = Path::new(object);
  if object.is_empty()
    || object.contains(['\\', ':'])
    || relative.components().any(|component| !matches!(component, Component::Normal(_)))
  {
    return Err(format!("Invalid vanilla artifact path: {object}").into());
  }
  Ok(vanilla_dir.join(relative))
}
