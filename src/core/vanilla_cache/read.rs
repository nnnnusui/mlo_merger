//! Validated derived-cache metadata for downstream processing stages.

use super::{CACHE_SCHEMA, Result, YmapRelationshipIndex};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

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
    revision: format!("{:x}", digest.finalize()),
  })
}
