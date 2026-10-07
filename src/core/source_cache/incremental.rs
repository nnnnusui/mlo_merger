use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  io::BufReader,
  path::{Component, Path},
};

use sha2::{Digest, Sha256};

use super::{
  BuildSourceCache, Result, Staging,
  inventory::{
    discover_resources, fingerprint, resource_id, source_fingerprints, source_inventory,
  },
  publication::{create_staging_directory, prepare_output_path},
  types::{
    FileFingerprint, ResourceCheck, ResourceInventory, SourceCacheConflictReport,
    SourceCacheMetadata, VanillaMatch,
  },
  vanilla::{revision, validate_derived_cache},
  ymap_plan,
};
use crate::core::{
  stream_conflicts::{StreamConflictReport, StreamFileConflict},
  vanilla::{load_manifest, write_json},
  vanilla_cache::BuildVanillaCache,
};

pub(super) fn run(
  build: &BuildSourceCache,
  selection: Option<&str>,
) -> Result<bool> {
  let source_dir = build.source_dir.canonicalize()?;
  let resources = discover_resources(&source_dir)?;
  let mut selected = Vec::new();
  let mut names = BTreeSet::new();
  for resource in &resources {
    let name = resource.file_name().ok_or("Resource has no name")?.to_string_lossy();
    if name == "_old" || !names.insert(name.to_ascii_lowercase()) {
      return Err(format!("Duplicate or reserved resource name: {name}").into());
    }
    let id = resource_id(&source_dir, resource)?;
    if selection.is_none_or(|selection| selection == name || selection == id) {
      selected.push(resource.clone());
    }
  }
  if selection.is_some() && selected.is_empty() {
    return Err(format!("Resource not found: {}", selection.unwrap_or_default()).into());
  }
  let vanilla_dir = build.vanilla_dir.canonicalize()?;
  BuildVanillaCache {
    vanilla_dir: vanilla_dir.clone(),
    output_dir: build.vanilla_cache_dir.clone(),
    through_version: None,
    force: false,
  }
  .run()?;
  let vanilla_cache_dir = build.vanilla_cache_dir.canonicalize()?;
  let output =
    prepare_output_path(&build.output_dir, &[&source_dir, &vanilla_dir, &vanilla_cache_dir])?;
  let previous = if output.join("source_cache_info.json").is_file() {
    let metadata: SourceCacheMetadata = serde_json::from_reader(BufReader::new(fs::File::open(
      output.join("source_cache_info.json"),
    )?))?;
    if metadata.source_dir != source_dir || !matches!(metadata.format_version, 2 | 3) {
      return Err("Source cache belongs to another input directory or unsupported schema".into());
    }
    Some(metadata)
  } else {
    None
  };
  let discovered = resources
    .iter()
    .map(|resource| resource_id(&source_dir, resource))
    .collect::<Result<BTreeSet<_>>>()?;
  let mut inventories =
    previous.as_ref().map(|metadata| metadata.resources.clone()).unwrap_or_default();
  let mut cached_inputs =
    previous.as_ref().map(|metadata| metadata.source_inputs.clone()).unwrap_or_default();
  let mut checks =
    previous.as_ref().map(|metadata| metadata.resource_checks.clone()).unwrap_or_default();
  let mut cached_names = BTreeMap::new();
  let mut normalized_names = BTreeSet::new();
  for (id, inventory) in &inventories {
    let name = inventory.source.file_name().ok_or("Cached resource has no name")?;
    if !normalized_names.insert(name.to_string_lossy().to_ascii_lowercase()) {
      return Err(format!("Ambiguous cached resource name: {}", name.to_string_lossy()).into());
    }
    cached_names.insert(name.to_os_string(), id.clone());
  }
  let mut relocated = false;
  for resource in &selected {
    let id = resource_id(&source_dir, resource)?;
    let name = resource.file_name().ok_or("Resource has no name")?;
    let Some(old_id) = cached_names.get(name) else {
      continue;
    };
    if old_id == &id || discovered.contains(old_id) {
      continue;
    }
    let mut inventory = inventories.remove(old_id).ok_or("Cached resource missing")?;
    inventory.source = resource.clone();
    inventories.insert(id.clone(), inventory);
    let prefix = format!("{old_id}/");
    let inputs =
      cached_inputs.keys().filter(|key| key.starts_with(&prefix)).cloned().collect::<Vec<_>>();
    for key in inputs {
      let fingerprint = cached_inputs.remove(&key).ok_or("Cached resource fingerprint missing")?;
      cached_inputs.insert(format!("{id}/{}", &key[prefix.len()..]), fingerprint);
    }
    if let Some(check) = checks.remove(old_id) {
      checks.insert(id.clone(), check);
    }
    relocated = true;
    log::info!("Relocated source resource {old_id} -> {id}");
  }
  let manifest_sha256 =
    format!("{:x}", Sha256::digest(fs::read(vanilla_dir.join("cache_info.json"))?));
  load_manifest(&vanilla_dir)?;
  let (derived, relationships) = validate_derived_cache(&vanilla_cache_dir, &manifest_sha256)?;
  let derived_revision = revision(&vanilla_cache_dir)?;
  let resource_revision = format!("{manifest_sha256}:{derived_revision}");
  let upstream_changed = previous.as_ref().is_none_or(|metadata| {
    metadata.format_version != 3
      || metadata.vanilla_dir != vanilla_dir
      || metadata.vanilla_cache_dir != vanilla_cache_dir
      || metadata.vanilla_manifest_sha256 != manifest_sha256
      || metadata.vanilla_cache_revision != derived_revision
  });
  let empty_inputs = BTreeMap::new();
  let current_inputs = source_fingerprints(
    &source_dir,
    &selected,
    if build.force { &empty_inputs } else { &cached_inputs },
  )?;
  let mut source_inputs = cached_inputs.clone();
  let generated_at = chrono::Utc::now().to_rfc3339();
  let history =
    output.join("_old").join(chrono::Utc::now().format("%Y%m%dT%H%M%S%.9fZ").to_string());
  let mut changed = relocated
    || upstream_changed
    || previous
      .as_ref()
      .is_some_and(|metadata| metadata.outputs.iter().any(|name| !output.join(name).is_file()));
  fs::create_dir_all(&output)?;
  ensure_safe_path(&output, &history)?;
  let staging = Staging(create_staging_directory(&output)?);
  for resource in &selected {
    let id = resource_id(&source_dir, resource)?;
    let prefix = format!("{id}/");
    let cache_name = resource.file_name().ok_or("Resource has no name")?.to_string_lossy();
    let current =
      current_inputs.iter().filter(|(key, _)| key.starts_with(&prefix)).collect::<BTreeMap<_, _>>();
    let cached =
      cached_inputs.iter().filter(|(key, _)| key.starts_with(&prefix)).collect::<BTreeMap<_, _>>();
    let previous_inventory = inventories.get(&id);
    let mut intact = previous_inventory.is_some();
    let mut layout_current = true;
    if let Some(inventory) = previous_inventory {
      for file in inventory.files_by_format.values().flatten() {
        let Some(path) = &file.cached_path else {
          continue;
        };
        let expected = format!("resources/{cache_name}/{}", stream_relative_path(&file.path)?);
        layout_current &= path == &expected;
        let cached_path = if path != &expected && !output.join(path).is_file() {
          output.join(&expected)
        } else {
          output.join(path)
        };
        ensure_safe_path(&output, &cached_path)?;
        if !cached_path.is_file() {
          if !resource.join(&file.path).is_file() {
            return Err(
              format!(
                "Moved source file is missing and cannot be regenerated: {}",
                cached_path.display()
              )
              .into(),
            );
          }
          intact = false;
          continue;
        }
        let recorded = super::types::FileFingerprint {
          sha256: file.sha256.clone(),
          size: file.size,
          modified_seconds: file.modified_seconds,
          modified_nanos: file.modified_nanos,
        };
        let current = fingerprint(&cached_path, if build.force { None } else { Some(&recorded) })?;
        intact &= current == recorded;
      }
    }
    let revision_current =
      checks.get(&id).is_some_and(|check| check.vanilla_revision == resource_revision);
    if !build.force
      && !upstream_changed
      && revision_current
      && current == cached
      && intact
      && layout_current
    {
      continue;
    }
    changed = true;
    log::info!("Updating source resource {id}");
    let mut inventory = source_inventory(
      &source_dir,
      std::slice::from_ref(resource),
      &current_inputs,
      &derived.files,
      if build.force { None } else { previous_inventory },
    )?
    .remove(&id)
    .ok_or("Resource inventory missing")?;
    let cache_root = output.join("resources").join(cache_name.as_ref());
    let mut retained = previous_inventory.cloned();
    let mut targets = BTreeMap::new();
    for file in
      inventory.files_by_format.values().flatten().filter(|file| file.vanilla.is_some()).chain(
        retained
          .iter()
          .flat_map(|inventory| inventory.files_by_format.values().flatten())
          .filter(|file| file.cached_path.is_some()),
      )
    {
      let relative = stream_relative_path(&file.path)?.to_ascii_lowercase();
      if let Some(previous) = targets.insert(relative.clone(), file.path.clone())
        && previous != file.path
      {
        return Err(
          format!(
            "Conflicting stream-relative cache paths in {id}: {previous} and {} -> {relative}",
            file.path
          )
          .into(),
        );
      }
    }
    if let Some(retained) = &mut retained {
      normalize_cached_layout(&output, &cache_name, retained)?;
      for (format, files) in &retained.files_by_format {
        for file in files {
          let Some(cached_path) = &file.cached_path else {
            continue;
          };
          let mut file = file.clone();
          if !inventory
            .files_by_format
            .values()
            .flatten()
            .any(|incoming| incoming.path == file.path)
          {
            let recorded = FileFingerprint {
              sha256: file.sha256.clone(),
              size: file.size,
              modified_seconds: file.modified_seconds,
              modified_nanos: file.modified_nanos,
            };
            let path = output.join(cached_path);
            let fingerprint = fingerprint(&path, if build.force { None } else { Some(&recorded) })?;
            if format == ".ymap" && (build.force || fingerprint.sha256 != file.sha256) {
              match crate::core::vanilla_cache::ymap_parent_hash(&path) {
                Ok(hash) => {
                  file.ymap_parent_hash = Some(format!("{hash:08X}"));
                  file.metadata_error = None;
                }
                Err(error) => {
                  file.ymap_parent_hash = None;
                  file.metadata_error = Some(error.to_string());
                }
              }
            }
            file.sha256 = fingerprint.sha256;
            file.size = fingerprint.size;
            file.modified_seconds = fingerprint.modified_seconds;
            file.modified_nanos = fingerprint.modified_nanos;
            inventory.files_by_format.entry(format.clone()).or_default().push(file);
          }
        }
      }
    }
    for file in inventory.files_by_format.values_mut().flatten() {
      if file.cached_path.is_some() || file.vanilla.is_none() {
        continue;
      }
      let relative = stream_relative_path(&file.path)?;
      let destination = cache_root.join(relative);
      ensure_safe_path(&output, &destination)?;
      if destination.exists() {
        fs::create_dir_all(&history)?;
        if !history.join("source_cache_info.json").exists()
          && let Some(previous) = &previous
        {
          write_json(&history.join("source_cache_info.json"), previous)?;
        }
        let archived = history.join(cache_name.as_ref()).join(relative);
        fs::create_dir_all(archived.parent().ok_or("Archive path has no parent")?)?;
        let old_fingerprint = fingerprint(&destination, None)?;
        write_json(
          &archived.with_file_name(format!(
            "{}.fingerprint.json",
            archived.file_name().unwrap().to_string_lossy()
          )),
          &old_fingerprint,
        )?;
        move_file(&destination, &archived)?;
      }
      fs::create_dir_all(destination.parent().ok_or("Cache file has no parent")?)?;
      move_file(&resource.join(&file.path), &destination)?;
      file.cached_path = Some(format!("resources/{cache_name}/{relative}"));
    }
    for files in inventory.files_by_format.values_mut() {
      files.sort_by(|left, right| left.path.cmp(&right.path));
    }
    inventories.insert(id.clone(), inventory);
    source_inputs.retain(|key, _| !key.starts_with(&prefix));
    source_inputs.extend(source_fingerprints(
      &source_dir,
      std::slice::from_ref(resource),
      &current_inputs,
    )?);
    checks.insert(
      id,
      ResourceCheck {
        checked_at: generated_at.clone(),
        vanilla_revision: resource_revision.clone(),
        has_stream: ["stream", "streams"].iter().any(|name| resource.join(name).is_dir()),
      },
    );
  }
  if selection.is_none() {
    let removed =
      inventories.keys().filter(|id| !discovered.contains(*id)).cloned().collect::<Vec<_>>();
    for id in removed {
      changed = true;
      if let Some(inventory) = inventories.remove(&id) {
        let name = inventory.source.file_name().ok_or("Resource has no name")?;
        if names.contains(&name.to_string_lossy().to_ascii_lowercase()) {
          return Err(
            format!(
              "Refusing to archive cache owned by a discovered resource: {}",
              name.to_string_lossy()
            )
            .into(),
          );
        }
        let current = output.join("resources").join(name);
        let cached = if current.exists() { current } else { output.join(name) };
        if cached.exists() {
          fs::create_dir_all(&history)?;
          if let Some(previous) = &previous {
            write_json(&history.join("source_cache_info.json"), previous)?;
          }
          ensure_safe_path(&output, &cached)?;
          fs::rename(cached, history.join(name))?;
        }
      }
      source_inputs.retain(|key, _| !key.starts_with(&format!("{id}/")));
      checks.remove(&id);
    }
  }
  let conflicts = conflicts(&source_dir, &output, &inventories)?;
  let previous_conflicts =
    fs::File::open(output.join("stream_conflicts.json")).ok().and_then(|file| {
      serde_json::from_reader::<_, SourceCacheConflictReport>(BufReader::new(file)).ok()
    });
  changed |= previous_conflicts.as_ref() != Some(&conflicts);
  if !changed {
    return Ok(false);
  }
  for inventory in inventories.values_mut() {
    for file in inventory.files_by_format.values_mut().flatten() {
      file.vanilla = derived.files.get(&file.file_name).map(|vanilla| VanillaMatch {
        version: vanilla.version.clone(),
        sha256: vanilla.sha256.clone(),
        content_matches: vanilla.sha256 == file.sha256,
      });
    }
  }
  let plan = ymap_plan::build(&inventories);
  let vanilla_ymaps =
    ymap_plan::vanilla_ymaps_to_read(&plan.changed_source_parents, &relationships, &derived.files);
  let outputs = BTreeSet::from([
    "source_cache.log".into(),
    "stream_conflicts.json".into(),
    "vanilla_ymaps_to_read.json".into(),
  ]);
  let metadata = SourceCacheMetadata {
    format_version: 3,
    source_dir,
    vanilla_dir,
    vanilla_cache_dir,
    vanilla_manifest_sha256: manifest_sha256,
    vanilla_cache_revision: derived_revision,
    latest_vanilla_version: derived.latest_version,
    generated_at,
    source_inputs,
    resource_checks: checks,
    resources: inventories,
    scanned_stream_file_count: conflicts.report.scanned_file_count,
    conflict_count: conflicts.report.conflict_count,
    ymap_load_plan: plan,
    outputs,
  };
  write_json(&staging.0.join("stream_conflicts.json"), &conflicts)?;
  write_json(&staging.0.join("vanilla_ymaps_to_read.json"), &vanilla_ymaps)?;
  fs::write(
    staging.0.join("source_cache.log"),
    format!(
      "generated_at={}\nresources={}\nstream_files={}\nconflicts={}\n",
      metadata.generated_at,
      metadata.resources.len(),
      metadata.scanned_stream_file_count,
      metadata.conflict_count
    ),
  )?;
  write_json(&staging.0.join("source_cache_info.json"), &metadata)?;
  for name in metadata.outputs.iter().map(String::as_str).chain(["source_cache_info.json"]) {
    fs::rename(staging.0.join(name), output.join(name))?;
  }
  Ok(true)
}

fn stream_relative_path(path: &str) -> Result<&str> {
  path
    .strip_prefix("stream/")
    .or_else(|| path.strip_prefix("streams/"))
    .filter(|relative| !relative.is_empty())
    .ok_or_else(|| format!("Source file is not under a stream directory: {path}").into())
}

/// Preserves original stream paths while migrating recorded cache paths without replacing files.
fn normalize_cached_layout(
  output: &Path,
  name: &str,
  inventory: &mut ResourceInventory,
) -> Result<()> {
  let mut moves = Vec::new();
  for file in inventory.files_by_format.values().flatten() {
    let Some(old) = &file.cached_path else {
      continue;
    };
    let new = format!("resources/{name}/{}", stream_relative_path(&file.path)?);
    if old != &new {
      ensure_safe_path(output, &output.join(old))?;
      ensure_safe_path(output, &output.join(&new))?;
      moves.push((old.clone(), new));
    }
  }
  let old_paths = moves.iter().map(|(old, _)| old.as_str()).collect::<BTreeSet<_>>();
  for (old, new) in &moves {
    if output.join(old).is_file() && output.join(new).exists() && !old_paths.contains(new.as_str())
    {
      return Err(
        format!("Cache layout migration would overwrite {}", output.join(new).display()).into(),
      );
    }
  }
  moves.sort_by_key(|(old, _)| Path::new(old).components().count());
  let mut directories = BTreeSet::new();
  for (old, new) in &moves {
    let source = output.join(old);
    let destination = output.join(new);
    if source.is_file() {
      if destination.exists() {
        return Err(
          format!("Cache layout migration would overwrite {}", destination.display()).into(),
        );
      }
      fs::create_dir_all(destination.parent().ok_or("Cache file has no parent")?)?;
      move_file(&source, &destination)?;
    }
    for parent in source.ancestors().skip(1).take_while(|parent| {
      parent.starts_with(output)
        && *parent != output
        && *parent != output.join("resources")
        && *parent != output.join("resources").join(name)
    }) {
      directories.insert(parent.to_path_buf());
    }
  }
  let mut directories = directories.into_iter().collect::<Vec<_>>();
  directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
  for directory in directories {
    if let Err(error) = fs::remove_dir(&directory)
      && !matches!(
        error.kind(),
        std::io::ErrorKind::DirectoryNotEmpty | std::io::ErrorKind::NotFound
      )
    {
      return Err(error.into());
    }
  }
  for file in inventory.files_by_format.values_mut().flatten() {
    if file.cached_path.is_some() {
      file.cached_path = Some(format!("resources/{name}/{}", stream_relative_path(&file.path)?));
    }
  }
  Ok(())
}

fn ensure_safe_path(
  root: &Path,
  path: &Path,
) -> Result<()> {
  let relative = path.strip_prefix(root)?;
  let mut current = root.to_path_buf();
  for component in relative.components() {
    if !matches!(component, Component::Normal(_)) {
      return Err("Unsafe source cache path".into());
    }
    current.push(component);
    if fs::symlink_metadata(&current).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
      return Err(format!("Refusing symlink cache path: {}", current.display()).into());
    }
  }
  Ok(())
}

fn move_file(
  source: &Path,
  destination: &Path,
) -> Result<()> {
  match fs::rename(source, destination) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => {
      let metadata = fs::metadata(source)?;
      fs::copy(source, destination)?;
      fs::File::open(destination)?
        .set_times(fs::FileTimes::new().set_modified(metadata.modified()?))?;
      fs::remove_file(source)?;
      Ok(())
    }
    Err(error) => Err(error.into()),
  }
}

fn conflicts(
  source: &Path,
  output: &Path,
  inventories: &BTreeMap<String, ResourceInventory>,
) -> Result<SourceCacheConflictReport> {
  let mut names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
  let mut scanned_file_count = 0;
  for inventory in inventories.values() {
    let name = inventory.source.file_name().ok_or("Resource has no name")?;
    let current = output.join("resources").join(name);
    let root = if current.is_dir() { current } else { output.join(name) };
    ensure_safe_path(output, &root)?;
    if !root.is_dir() {
      continue;
    }
    for entry in walkdir::WalkDir::new(&root).follow_links(false) {
      let entry = entry?;
      if !entry.file_type().is_file() {
        continue;
      }
      let path = entry.path();
      let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Cache filename is not UTF-8")?
        .to_ascii_lowercase();
      let relative = path.strip_prefix(output)?.to_string_lossy().replace('\\', "/");
      scanned_file_count += 1;
      names.entry(name).or_default().insert(relative);
    }
  }
  let conflicts = group_conflicts(names);
  Ok(SourceCacheConflictReport {
    report: StreamConflictReport {
      input_dir: output.to_string_lossy().into_owned(),
      scanned_file_count,
      conflict_count: conflicts.values().map(Vec::len).sum(),
      conflicts,
    },
    source_conflicts: Some(crate::core::stream_conflicts::scan_stream_conflicts(source)?.conflicts),
  })
}

fn group_conflicts(
  names: BTreeMap<String, BTreeSet<String>>
) -> BTreeMap<String, Vec<StreamFileConflict>> {
  let mut conflicts: BTreeMap<String, Vec<StreamFileConflict>> = BTreeMap::new();
  for (file_name, paths) in names {
    if paths.len() < 2 {
      continue;
    }
    let extension = Path::new(&file_name)
      .extension()
      .and_then(|value| value.to_str())
      .map(|value| format!(".{value}"))
      .unwrap_or_else(|| "[no extension]".into());
    conflicts.entry(extension).or_default().push(StreamFileConflict {
      file_name,
      paths: paths.into_iter().collect(),
    });
  }
  conflicts
}
