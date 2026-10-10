use std::{collections::BTreeSet, fs, io::BufReader, path::Path};

use super::{
  Result,
  incremental::{ensure_safe_path, move_file},
  types::SourceCacheMetadata,
};

/// Moves recorded active cache files back to their original resource-relative paths.
///
/// All paths and existing destinations are checked before moving any files.
/// Different destination contents, missing inputs and symlinks are errors.
/// Identical destination files are retained. Successful restoration invalidates
/// source-cache indexes; history and unrecorded cache files remain untouched.
///
/// ```no_run
/// let restored = mlo_merger::core::source_cache::restore_source_cache(
///   std::path::Path::new("asset/source-cache"),
///   std::path::Path::new("asset/source"),
/// )?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn restore_source_cache(
  cache_dir: &Path,
  source_dir: &Path,
) -> Result<usize> {
  let cache = cache_dir.canonicalize()?;
  let source = source_dir.canonicalize()?;
  if cache.starts_with(&source) || source.starts_with(&cache) {
    return Err("Source and source-cache directories overlap".into());
  }
  let reports = ["source_cache_info.json", "stream_conflicts.json", "vanilla_ymaps_to_read.json"];
  for report in reports {
    ensure_safe_path(&cache, &cache.join(report))?;
    let path = cache.join(report);
    if path.try_exists()? && !path.is_file() {
      return Err(format!("Source-cache index is not a file: {}", path.display()).into());
    }
  }
  let manifest = cache.join(reports[0]);
  if !manifest.exists() {
    return Ok(0);
  }
  let metadata: SourceCacheMetadata =
    serde_json::from_reader(BufReader::new(fs::File::open(&manifest)?))?;
  if !matches!(metadata.format_version, 2 | 3) {
    return Err(format!("Unsupported source cache schema {}", metadata.format_version).into());
  }
  let mut planned = Vec::new();
  let mut cached_paths = BTreeSet::new();
  let mut destinations = BTreeSet::new();
  for resource in metadata.resources.values() {
    if !resource.files_by_format.values().flatten().any(|file| file.cached_path.is_some()) {
      continue;
    }
    let relative_resource = resource.source.strip_prefix(&metadata.source_dir)?;
    let resource_dir = source.join(relative_resource);
    ensure_safe_path(&source, &resource_dir)?;
    if !resource_dir.is_dir() {
      return Err(format!("Source resource is missing: {}", resource_dir.display()).into());
    }
    for file in resource.files_by_format.values().flatten() {
      let Some(relative_cache) = &file.cached_path else {
        continue;
      };
      let relative_cache = Path::new(relative_cache);
      if relative_cache.starts_with("_old") {
        return Err("Refusing to restore archived source-cache files".into());
      }
      let name = resource.source.file_name().ok_or("Resource has no name")?;
      let active = Path::new("resources").join(name);
      if !relative_cache.starts_with(&active) && !relative_cache.starts_with(Path::new(name)) {
        return Err("Cached file is outside its resource directory".into());
      }
      let cached = cache.join(relative_cache);
      let original = Path::new(&file.path);
      if original.components().any(|part| !matches!(part, std::path::Component::Normal(_)))
        || original.as_os_str().is_empty()
      {
        return Err("Unsafe original source path".into());
      }
      let destination = resource_dir.join(original);
      ensure_safe_path(&cache, &cached)?;
      ensure_safe_path(&source, &destination)?;
      if !cached_paths.insert(cached.clone()) || !destinations.insert(destination.clone()) {
        return Err("Duplicate source-cache restoration path".into());
      }
      if !cached.is_file() {
        if destination.is_file() {
          let bytes = fs::read(&destination)?;
          use sha2::{Digest, Sha256};
          if format!("{:x}", Sha256::digest(&bytes)) == file.sha256 {
            continue;
          }
        }
        return Err(format!("Cached source file is missing: {}", cached.display()).into());
      }
      let existing = destination.try_exists()?;
      if existing && (!destination.is_file() || fs::read(&cached)? != fs::read(&destination)?) {
        return Err(
          format!("Restore destination has different contents: {}", destination.display()).into(),
        );
      }
      planned.push((cached, destination, existing));
    }
  }
  for (cached, destination, existing) in &planned {
    if *existing {
      fs::remove_file(cached)?;
    } else {
      fs::create_dir_all(destination.parent().ok_or("Restore destination has no parent")?)?;
      move_file(cached, destination)?;
    }
  }
  for report in reports {
    match fs::remove_file(cache.join(report)) {
      Ok(()) => {}
      Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
      Err(error) => return Err(error.into()),
    }
  }
  Ok(planned.len())
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn restores_original_paths_and_rejects_conflicts_before_moving() {
    let root = std::env::temp_dir().join(format!("restore_source_cache_{}", std::process::id()));
    let source = root.join("source");
    let cache = root.join("cache");
    fs::create_dir_all(source.join("[group]/resource/stream")).unwrap();
    fs::create_dir_all(cache.join("resources/resource")).unwrap();
    fs::create_dir_all(cache.join("_old")).unwrap();
    let cached = cache.join("resources/resource/map.ymap");
    let original = source.join("[group]/resource/stream/map.ymap");
    fs::write(&cached, b"native bytes").unwrap();
    let modified = fs::metadata(&cached).unwrap().modified().unwrap();
    fs::write(cache.join("_old/history"), b"history").unwrap();
    fs::write(cache.join("resources/resource/unrecorded"), b"keep").unwrap();
    let mut manifest = json!({
      "format_version": 3, "source_dir": source, "vanilla_dir": "vanilla",
      "vanilla_cache_dir": "vanilla-cache", "vanilla_manifest_sha256": "hash",
      "vanilla_cache_revision": "hash", "latest_vanilla_version": "base",
      "generated_at": "now", "source_inputs": {}, "resource_checks": {},
      "resources": {"[group]/resource": {
        "source": source.join("[group]/resource"), "files_by_format": {".ymap": [{
          "path": "stream/map.ymap", "file_name": "map.ymap", "sha256": "hash",
          "size": 12, "modified_seconds": 0, "modified_nanos": 0,
          "vanilla": null, "ymap_parent_hash": null, "metadata_error": null,
          "cached_path": "resources/resource/map.ymap"
        }]}
      }},
      "scanned_stream_file_count": 1, "conflict_count": 0,
      "ymap_load_plan": {"changed_source_parents": [], "additional_source_children": []},
      "outputs": []
    });
    let manifest_path = cache.join("source_cache_info.json");
    let second_cached = cache.join("resources/resource/collision.ybn");
    let second_original = source.join("[group]/resource/stream/nested/collision.ybn");
    fs::create_dir_all(second_original.parent().unwrap()).unwrap();
    fs::write(&second_cached, b"collision bytes").unwrap();
    let mut second_file =
      manifest["resources"]["[group]/resource"]["files_by_format"][".ymap"][0].clone();
    second_file["path"] = json!("stream/nested/collision.ybn");
    second_file["file_name"] = json!("collision.ybn");
    second_file["cached_path"] = json!("resources/resource/collision.ybn");
    manifest["resources"]["[group]/resource"]["files_by_format"][".ybn"] = json!([second_file]);
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    fs::write(&second_original, b"different").unwrap();
    assert!(restore_source_cache(&cache, &source).is_err());
    assert!(cached.is_file());
    assert!(!original.exists());
    assert!(manifest_path.is_file());
    fs::write(&second_original, b"collision bytes").unwrap();
    let mut unsafe_manifest = manifest.clone();
    unsafe_manifest["resources"]["[group]/resource"]["files_by_format"][".ymap"][0]["path"] =
      json!("../escape.ymap");
    fs::write(&manifest_path, serde_json::to_vec(&unsafe_manifest).unwrap()).unwrap();
    assert!(restore_source_cache(&cache, &source).is_err());
    assert!(cached.is_file());
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    #[cfg(unix)]
    {
      std::os::unix::fs::symlink(&cached, &original).unwrap();
      assert!(restore_source_cache(&cache, &source).is_err());
      assert!(cached.is_file());
      fs::remove_file(&original).unwrap();
    }
    assert!(restore_source_cache(&cache, &cache).is_err());
    fs::write(cache.join("stream_conflicts.json"), b"{}").unwrap();
    fs::write(cache.join("vanilla_ymaps_to_read.json"), b"[]").unwrap();
    assert_eq!(restore_source_cache(&cache, &source).unwrap(), 2);
    assert_eq!(fs::read(&original).unwrap(), b"native bytes");
    assert_eq!(fs::metadata(&original).unwrap().modified().unwrap(), modified);
    assert!(!cached.exists());
    assert!(!manifest_path.exists());
    assert!(!second_cached.exists());
    assert_eq!(fs::read(&second_original).unwrap(), b"collision bytes");
    assert!(!cache.join("stream_conflicts.json").exists());
    assert!(!cache.join("vanilla_ymaps_to_read.json").exists());
    assert!(cache.join("_old/history").is_file());
    assert!(cache.join("resources/resource/unrecorded").is_file());
    assert_eq!(restore_source_cache(&cache, &source).unwrap(), 0);
    fs::remove_dir_all(root).unwrap();
  }
}
