//! Validated derived-cache metadata for downstream processing stages.

use super::{CACHE_SCHEMA, Result, YmapRelationshipIndex};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
  collections::BTreeMap,
  fs,
  path::{Path, PathBuf},
};

#[derive(Debug, Deserialize)]
pub(crate) struct DerivedVanillaFile {
  pub(crate) version: String,
  pub(crate) sha256: String,
  pub(crate) object: String,
}

#[derive(Debug, Deserialize)]
struct Metadata {
  format_version: u32,
  vanilla_manifest_sha256: String,
  latest_version: String,
  files: BTreeMap<String, DerivedVanillaFile>,
}

/// The derived cache's metadata, written whenever the cache is rebuilt, so it changes with every
/// update of the latest files.
pub fn cache_info_path(cache_dir: &Path) -> PathBuf {
  cache_dir.join("cache_info.json")
}

/// One latest vanilla stream file of the derived cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaLatestFile {
  /// Stream file name, such as `hei_ch1_11.ymap`.
  pub name: String,
  /// The vanilla version the file comes from.
  pub version: String,
  /// The cached file, below the cache directory.
  pub path: PathBuf,
}

/// Lists the latest vanilla stream files recorded in the derived cache's metadata, without
/// checking it against the raw archive.
pub fn latest_files(cache_dir: &Path) -> Result<Vec<VanillaLatestFile>> {
  let path = cache_info_path(cache_dir);
  let metadata: Metadata = serde_json::from_slice(
    &fs::read(&path).map_err(|error| format!("Cannot open {}: {error}", path.display()))?,
  )?;
  if metadata.format_version != CACHE_SCHEMA {
    return Err(format!("Unsupported vanilla-cache schema {}", metadata.format_version).into());
  }
  metadata
    .files
    .into_iter()
    .map(|(name, file)| {
      let object = Path::new(&file.object);
      if object.is_absolute()
        || object.components().any(|part| !matches!(part, std::path::Component::Normal(_)))
      {
        return Err(format!("Unsafe vanilla-cache artifact path: {}", file.object).into());
      }
      Ok(VanillaLatestFile {
        name,
        version: file.version,
        path: cache_dir.join(object),
      })
    })
    .collect()
}

/// Validated metadata and a revision covering the exact manifest/index bytes read.
pub(crate) struct LoadedCache {
  pub(crate) latest_version: String,
  pub(crate) files: BTreeMap<String, DerivedVanillaFile>,
  pub(crate) relationships: YmapRelationshipIndex,
  pub(crate) revision: String,
}

/// Loads each metadata file once, validating its schema, provenance and artifacts.
pub(crate) fn load_cache(
  directory: &Path,
  raw_revision: &str,
) -> Result<LoadedCache> {
  let metadata_bytes = fs::read(directory.join("cache_info.json"))?;
  let relationship_bytes = fs::read(directory.join("ymap_relationships.json"))?;
  let metadata: Metadata = serde_json::from_slice(&metadata_bytes)?;
  if metadata.format_version != CACHE_SCHEMA || metadata.vanilla_manifest_sha256 != raw_revision {
    return Err("Derived vanilla cache is stale or has an unsupported schema".into());
  }
  let relationships: YmapRelationshipIndex = serde_json::from_slice(&relationship_bytes)?;
  if relationships.format_version != CACHE_SCHEMA
    || relationships.vanilla_manifest_sha256 != raw_revision
    || relationships.version != metadata.latest_version
  {
    return Err("Derived vanilla YMAP relationship index is stale".into());
  }
  for format in ["ymap", "ybn"] {
    if !directory.join("latest").join(format).is_dir() {
      return Err(format!("Derived vanilla cache is missing latest/{format}").into());
    }
  }
  for file in metadata.files.values() {
    if !directory.join(&file.object).is_file() {
      return Err(format!("Derived vanilla latest file is missing: {}", file.object).into());
    }
  }
  let mut digest = Sha256::new();
  digest.update(b"cache_info.json");
  digest.update(&metadata_bytes);
  digest.update(b"ymap_relationships.json");
  digest.update(&relationship_bytes);
  Ok(LoadedCache {
    latest_version: metadata.latest_version,
    files: metadata.files,
    relationships,
    revision: hex::encode(digest.finalize()),
  })
}
