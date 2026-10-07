use std::{fs, io::BufReader, path::Path};

use sha2::{Digest, Sha256};

use super::{
  Result,
  types::{DerivedVanillaCache, YmapRelationshipIndex},
};

pub(super) fn validate_derived_cache(
  directory: &Path,
  raw_revision: &str,
) -> Result<(DerivedVanillaCache, YmapRelationshipIndex)> {
  let metadata: DerivedVanillaCache =
    serde_json::from_reader(BufReader::new(fs::File::open(directory.join("cache_info.json"))?))?;
  if metadata.format_version != 2 || metadata.vanilla_manifest_sha256 != raw_revision {
    return Err("Derived vanilla cache is stale or has an unsupported schema".into());
  }
  let relationships: YmapRelationshipIndex = serde_json::from_reader(BufReader::new(
    fs::File::open(directory.join("ymap_relationships.json"))?,
  ))?;
  if relationships.format_version != 2
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
  Ok((metadata, relationships))
}

pub(super) fn revision(directory: &Path) -> Result<String> {
  let mut hasher = Sha256::new();
  for file in ["cache_info.json", "ymap_relationships.json"] {
    hasher.update(file.as_bytes());
    hasher.update(fs::read(directory.join(file))?);
  }
  Ok(format!("{:x}", hasher.finalize()))
}
