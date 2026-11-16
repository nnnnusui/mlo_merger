//! The deployment's record of the files it copied (`deploy_cache_info.json`).

use super::Result;
use crate::platform::fs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
  collections::BTreeMap,
  io::{BufReader, Read},
  path::{Path, PathBuf},
};

/// The metadata file at the root of a deployment.
pub(super) const METADATA_FILE: &str = "deploy_cache_info.json";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[cfg_attr(feature = "typescript", specta(rename = "DeployFingerprint"))]
pub(super) struct Fingerprint {
  pub(super) sha256: String,
  pub(super) size: u64,
  pub(super) modified_seconds: u64,
  pub(super) modified_nanos: u32,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(super) struct CopiedFile {
  /// The copied file relative to its store: the merge output for `merged/` files, the source
  /// cache for `clone/` files.
  pub(super) source: PathBuf,
  pub(super) input: Fingerprint,
  pub(super) output: Fingerprint,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(super) struct DeployMetadata {
  pub(super) format_version: u32,
  pub(super) generated_at: String,
  pub(super) files: BTreeMap<String, CopiedFile>,
}

/// Reads the deployment metadata at `path`, rejecting layouts this version cannot read.
pub(super) fn read_metadata(path: &Path) -> Result<DeployMetadata> {
  let metadata: DeployMetadata = crate::common::json::read_json(path)?;
  if metadata.format_version != 1 {
    return Err(format!("Unsupported deployment cache schema {}", metadata.format_version).into());
  }
  Ok(metadata)
}

/// The file at `path` as it is now; hashed again only when its size or modification time
/// differs from `cached`, or when `force`.
pub(super) fn fingerprint(
  path: &Path,
  cached: Option<&Fingerprint>,
  force: bool,
) -> Result<Fingerprint> {
  let metadata = fs::metadata(path)?;
  let modified = metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?;
  if !force
    && let Some(cached) = cached
    && cached.size == metadata.len()
    && cached.modified_seconds == modified.as_secs()
    && cached.modified_nanos == modified.subsec_nanos()
  {
    return Ok(cached.clone());
  }
  let mut reader = BufReader::new(fs::File::open(path)?);
  let mut digest = Sha256::new();
  let mut buffer = [0u8; 65536];
  loop {
    let count = reader.read(&mut buffer)?;
    if count == 0 {
      break;
    }
    digest.update(&buffer[..count]);
  }
  Ok(Fingerprint {
    sha256: hex::encode(digest.finalize()),
    size: metadata.len(),
    modified_seconds: modified.as_secs(),
    modified_nanos: modified.subsec_nanos(),
  })
}
