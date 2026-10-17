//! Resolves recorded stream inputs without rediscovering resources or untracked files.

use super::{Result, types::SourceCacheMetadata};
use crate::common::{
  filesystem::read_bytes,
  path::{reject_symlinks, safe_relative},
};
use std::{
  collections::BTreeMap,
  fs,
  io::BufReader,
  path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub(crate) struct StreamFile {
  pub path: PathBuf,
  pub original: PathBuf,
  pub parent_hint: Option<u32>,
}

impl StreamFile {
  /// Reads an active stream input through the source-cache filesystem boundary.
  pub(crate) fn read(&self) -> Result<Vec<u8>> {
    read_stream_bytes(&self.path)
  }
}

pub(crate) type StreamInventory = BTreeMap<String, BTreeMap<String, StreamFile>>;

/// Loads only resource/file identities recorded in the active source cache.
pub(crate) fn load_stream_inventory(
  source: &Path,
  cache: &Path,
) -> Result<StreamInventory> {
  let root = source.canonicalize()?;
  let metadata_path = cache.join("source_cache_info.json");
  reject_symlinks(&metadata_path)?;
  let metadata: SourceCacheMetadata =
    serde_json::from_reader(BufReader::new(fs::File::open(metadata_path)?))?;
  if !matches!(metadata.format_version, 2 | 3) {
    return Err("Unsupported source cache schema".into());
  }
  if metadata.source_dir.canonicalize()? != root {
    return Err("Source cache belongs to another source root".into());
  }
  let mut inventory = StreamInventory::new();
  for (id, resource) in metadata.resources {
    safe_relative(&id)?;
    reject_symlinks(&resource.source)?;
    if !resource.source.canonicalize()?.starts_with(&root) {
      return Err("Recorded resource is outside source root".into());
    }
    let mut files = BTreeMap::new();
    let mut hashes = BTreeMap::new();
    for file in resource.files_by_format.get(".ymap").into_iter().flatten() {
      safe_relative(&file.path)?;
      let relative = file
        .path
        .strip_prefix("stream/")
        .or_else(|| file.path.strip_prefix("streams/"))
        .ok_or("Recorded YMAP is outside a stream directory")?;
      safe_relative(relative)?;
      let original = resource.source.join(&file.path);
      reject_symlinks(&original)?;
      let path = if let Some(cached) = &file.cached_path {
        safe_relative(cached)?;
        if Path::new(cached).starts_with("_old") {
          return Err("Archived stream input is not active".into());
        }
        if original.exists() {
          return Err(format!("Both live and cached stream input: {id}/{relative}").into());
        }
        cache.join(cached)
      } else {
        original.clone()
      };
      reject_symlinks(&path)?;
      let stats = fs::metadata(&path)
        .map_err(|error| format!("Missing recorded stream input {}: {error}", path.display()))?;
      if !stats.is_file() {
        return Err("Recorded stream input is not a file".into());
      }
      let time = stats.modified()?.duration_since(std::time::UNIX_EPOCH)?;
      let parent_hint = if stats.len() == file.size
        && time.as_secs() == file.modified_seconds
        && time.subsec_nanos() == file.modified_nanos
      {
        file.ymap_parent_hash.as_ref().and_then(|hash| u32::from_str_radix(hash, 16).ok())
      } else {
        None
      };
      let stem = Path::new(relative)
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or("Invalid recorded YMAP name")?;
      let hash = crate::core::format::ymap::reference_hash(stem);
      if hashes.insert(hash, relative.to_owned()).is_some() {
        return Err("Ambiguous YMAP name/hash within recorded resource".into());
      }
      if files
        .insert(
          relative.to_owned(),
          StreamFile {
            path,
            original,
            parent_hint,
          },
        )
        .is_some()
      {
        return Err("Ambiguous recorded stream/streams path".into());
      }
    }
    inventory.insert(id, files);
  }
  Ok(inventory)
}

pub(crate) fn read_stream_bytes(path: &Path) -> Result<Vec<u8>> {
  read_bytes(path)
}

/// Resolves a generated/vanilla template whose latest-file entry may be a symlink.
pub(crate) fn resolve_template_file(path: &Path) -> Result<StreamFile> {
  let resolved = path.canonicalize()?;
  reject_symlinks(&resolved)?;
  Ok(StreamFile {
    path: resolved,
    original: path.to_path_buf(),
    parent_hint: None,
  })
}

#[cfg(test)]
pub(crate) fn record_test_inventory(
  source: &Path,
  cache: &Path,
  excluded: &[&str],
) -> Result<()> {
  use super::types::{ResourceInventory, SourceFile, YmapLoadPlan};
  use std::collections::BTreeSet;
  let root = source.canonicalize()?;
  let discovered = super::stream_discovery::discover_inventory(&root, cache)?;
  let mut resources = BTreeMap::new();
  for (id, files) in discovered {
    if excluded.contains(&id.as_str()) {
      continue;
    }
    let mut records = Vec::new();
    let resource_dir = if root.file_name().and_then(|name| name.to_str()) == Some(&id) {
      root.clone()
    } else {
      root.join(&id)
    };
    for file in files.values() {
      let fingerprint = crate::common::fingerprint::Fingerprint::read(&file.path, None, true)?;
      records.push(SourceFile {
        path: file.original.strip_prefix(&resource_dir)?.to_string_lossy().replace('\\', "/"),
        file_name: file
          .original
          .file_name()
          .and_then(|name| name.to_str())
          .ok_or("Invalid fixture stream")?
          .into(),
        sha256: fingerprint.sha256,
        size: fingerprint.size,
        modified_seconds: fingerprint.modified_seconds,
        modified_nanos: fingerprint.modified_nanos,
        vanilla: None,
        ymap_parent_hash: None,
        metadata_error: None,
        cached_path: if file.path != file.original {
          Some(file.path.strip_prefix(cache)?.to_string_lossy().replace('\\', "/"))
        } else {
          None
        },
      });
    }
    resources.insert(
      id,
      ResourceInventory {
        source: resource_dir,
        files_by_format: BTreeMap::from([(".ymap".into(), records)]),
      },
    );
  }
  fs::create_dir_all(cache)?;
  crate::core::vanilla::write_json(
    &cache.join("source_cache_info.json"),
    &SourceCacheMetadata {
      format_version: 3,
      source_dir: root,
      vanilla_dir: cache.join("vanilla"),
      vanilla_cache_dir: cache.join("vanilla-cache"),
      vanilla_manifest_sha256: String::new(),
      vanilla_cache_revision: String::new(),
      latest_vanilla_version: String::new(),
      generated_at: String::new(),
      source_inputs: BTreeMap::new(),
      resource_checks: BTreeMap::new(),
      resources,
      scanned_stream_file_count: 0,
      conflict_count: 0,
      ymap_load_plan: YmapLoadPlan {
        changed_source_parents: BTreeSet::new(),
        additional_source_children: BTreeSet::new(),
      },
      outputs: BTreeSet::new(),
    },
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn recorded_catalog_controls_live_and_cached_reads_and_rejects_unsafe_inputs() {
    let root = std::env::temp_dir().join(format!("recorded_streams_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let source = root.join("source");
    let cache = root.join("cache");
    fs::create_dir_all(source.join("[group]/resource/stream/nested")).unwrap();
    fs::write(source.join("[group]/resource/fxmanifest.lua"), []).unwrap();
    let live = source.join("[group]/resource/stream/nested/map.ymap");
    fs::write(&live, b"native fixture").unwrap();
    record_test_inventory(&source, &cache, &[]).unwrap();
    fs::create_dir_all(source.join("unrecorded/stream")).unwrap();
    fs::write(source.join("unrecorded/fxmanifest.lua"), []).unwrap();
    fs::write(source.join("unrecorded/stream/map.ymap"), b"ignore resource").unwrap();
    fs::write(source.join("[group]/resource/stream/unrecorded.ymap"), b"ignore file").unwrap();
    let inventory = load_stream_inventory(&source, &cache).unwrap();
    assert_eq!(inventory.len(), 1);
    assert_eq!(inventory["[group]/resource"].len(), 1);
    assert_eq!(inventory["[group]/resource"]["nested/map.ymap"].read().unwrap(), b"native fixture");
    let metadata_path = cache.join("source_cache_info.json");
    let mut metadata: SourceCacheMetadata =
      serde_json::from_reader(fs::File::open(&metadata_path).unwrap()).unwrap();
    let cached = cache.join("resources/resource/nested/map.ymap");
    fs::create_dir_all(cached.parent().unwrap()).unwrap();
    fs::rename(&live, &cached).unwrap();
    metadata
      .resources
      .get_mut("[group]/resource")
      .unwrap()
      .files_by_format
      .get_mut(".ymap")
      .unwrap()[0]
      .cached_path = Some("resources/resource/nested/map.ymap".into());
    crate::core::vanilla::write_json(&metadata_path, &metadata).unwrap();
    let inventory = load_stream_inventory(&source, &cache).unwrap();
    let resolved = &inventory["[group]/resource"]["nested/map.ymap"];
    assert_eq!(resolved.path, cached);
    assert_eq!(resolved.original, live);
    assert_eq!(resolved.read().unwrap(), b"native fixture");
    fs::write(&live, b"duplicate live").unwrap();
    assert!(
      load_stream_inventory(&source, &cache)
        .unwrap_err()
        .to_string()
        .contains("Both live and cached")
    );
    fs::remove_file(&live).unwrap();
    let other_source = root.join("other-source");
    fs::create_dir_all(&other_source).unwrap();
    assert!(
      load_stream_inventory(&other_source, &cache)
        .unwrap_err()
        .to_string()
        .contains("another source root")
    );
    fs::remove_file(&cached).unwrap();
    assert!(
      load_stream_inventory(&source, &cache)
        .unwrap_err()
        .to_string()
        .contains("Missing recorded stream input")
    );
    fs::write(&cached, b"native fixture").unwrap();
    #[cfg(unix)]
    {
      let target = root.join("link-target.ymap");
      fs::write(&target, b"linked input").unwrap();
      fs::remove_file(&cached).unwrap();
      std::os::unix::fs::symlink(&target, &cached).unwrap();
      assert!(load_stream_inventory(&source, &cache).unwrap_err().to_string().contains("symlink"));
      fs::remove_file(&cached).unwrap();
      fs::write(&cached, b"native fixture").unwrap();
    }
    metadata
      .resources
      .get_mut("[group]/resource")
      .unwrap()
      .files_by_format
      .get_mut(".ymap")
      .unwrap()[0]
      .cached_path = Some("../escape.ymap".into());
    crate::core::vanilla::write_json(&metadata_path, &metadata).unwrap();
    assert!(load_stream_inventory(&source, &cache).is_err());
    metadata
      .resources
      .get_mut("[group]/resource")
      .unwrap()
      .files_by_format
      .get_mut(".ymap")
      .unwrap()[0]
      .cached_path = Some("_old/map.ymap".into());
    crate::core::vanilla::write_json(&metadata_path, &metadata).unwrap();
    assert!(load_stream_inventory(&source, &cache).unwrap_err().to_string().contains("Archived"));
    metadata.resources.clear();
    crate::core::vanilla::write_json(&metadata_path, &metadata).unwrap();
    assert!(load_stream_inventory(&source, &cache).unwrap().is_empty());
    fs::remove_file(metadata_path).unwrap();
    assert!(load_stream_inventory(&source, &cache).is_err());
    fs::remove_dir_all(root).unwrap();
  }
}
