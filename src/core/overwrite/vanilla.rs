//! Differences written directly against the latest vanilla files.

use super::OverwriteFingerprint;
use crate::common::Result;
use crate::core::format::ymap::compare::YmapDiffFields;
use crate::fs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

/// The extension of a vanilla overwrite document, after the stream file's own name.
const DOCUMENT_EXTENSION: &str = ".diff.json";

/// Differences of one latest vanilla stream file, read from
/// `vanilla/latest/<file type>/<file name>.diff.json`.
///
/// They are merged as one more source after every resource's, so the merge policy decides
/// between them and the resources' changes to the same items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct VanillaOverwrite {
  /// Version of the document layout.
  pub schema_version: u32,
  /// The vanilla file these differences were written against.
  pub vanilla: OverwriteFingerprint,
  /// The differences from that vanilla file, in the shape of a diff document's `diffs`.
  pub diffs: YmapDiffFields,
}

impl VanillaOverwrite {
  /// The current [`Self::schema_version`].
  pub const SCHEMA_VERSION: u32 = 1;

  /// A digest of this overwrite, for telling whether a merge used the current one.
  pub fn digest(&self) -> String {
    hex::encode(Sha256::digest(serde_json::to_vec(self).unwrap_or_default()))
  }

  /// These differences when `vanilla` is the file they were written against; otherwise `None`.
  /// `label` names the file in the debug log.
  pub fn diffs_for(
    &self,
    vanilla: &OverwriteFingerprint,
    label: &str,
  ) -> Option<&YmapDiffFields> {
    // `Merge` reports this with every other overwrite it leaves out.
    if self.vanilla != *vanilla {
      tracing::debug!(
        "Vanilla overwrite of {label} was written against another vanilla file; not applied"
      );
      return None;
    }
    Some(&self.diffs)
  }
}

/// Reads the YMAP overwrites under `<directory>/vanilla`, keyed by lowercase file name.
///
/// Only `latest` is read, since merges compare sources with the latest vanilla files; other
/// versions and file types are reported and ignored.
pub(super) fn load(directory: &Path) -> Result<BTreeMap<String, VanillaOverwrite>> {
  let vanilla_dir = directory.join("vanilla");
  let mut overwrites = BTreeMap::new();
  if !fs::is_dir(&vanilla_dir) {
    return Ok(overwrites);
  }
  for entry in fs::read_dir(&vanilla_dir)? {
    let path = entry?.path();
    if path.file_name().is_some_and(|name| name != "latest") {
      tracing::warn!("Vanilla overwrites are read from latest only; ignoring {}", path.display());
    }
  }
  let latest = vanilla_dir.join("latest");
  if !fs::is_dir(&latest) {
    return Ok(overwrites);
  }
  for entry in fs::read_dir(&latest)? {
    let path = entry?.path();
    if path.file_name().is_none_or(|name| name != "ymap") {
      tracing::warn!("Only YMAP vanilla overwrites are supported; ignoring {}", path.display());
    }
  }
  let ymap_dir = latest.join("ymap");
  if !fs::is_dir(&ymap_dir) {
    return Ok(overwrites);
  }
  for entry in fs::read_dir(&ymap_dir)? {
    let path = entry?.path();
    let name = path
      .file_name()
      .and_then(|name| name.to_str())
      .ok_or_else(|| format!("Overwrite file name is not UTF-8: {}", path.display()))?
      .to_ascii_lowercase();
    let Some(stream_name) = name.strip_suffix(DOCUMENT_EXTENSION).filter(|name| {
      fs::is_file(&path) && Path::new(name).extension().is_some_and(|extension| extension == "ymap")
    }) else {
      tracing::warn!("Not a YMAP vanilla overwrite; ignoring {}", path.display());
      continue;
    };
    let overwrite: VanillaOverwrite = crate::common::json::read_json(&path)
      .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    if overwrite.schema_version != VanillaOverwrite::SCHEMA_VERSION {
      return Err(
        format!(
          "{} has schema version {}; expected {}",
          path.display(),
          overwrite.schema_version,
          VanillaOverwrite::SCHEMA_VERSION
        )
        .into(),
      );
    }
    overwrites.insert(stream_name.to_string(), overwrite);
  }
  tracing::info!(
    "Loaded vanilla overwrites of {} YMAP files from {}",
    overwrites.len(),
    ymap_dir.display()
  );
  Ok(overwrites)
}
