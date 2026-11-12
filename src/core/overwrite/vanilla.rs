//! Differences written directly against the latest vanilla files.

use super::OverwriteFingerprint;
use crate::common::Result;
use crate::core::format::ymap::compare::YmapDiffFields;
use crate::platform::fs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

/// The extension of a vanilla overwrite document, after the stream file's own name.
const DOCUMENT_EXTENSION: &str = ".diff.json";

/// Differences of one vanilla stream file, read from
/// `vanilla/<version>/<file type>/<file name>.diff.json`, the file's path in the vanilla archive
/// with `.diff.json` appended. It applies while that file is the latest one of its name.
///
/// They are merged as one more source after every resource's, so the merge policy decides
/// between them and the resources' changes to the same items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct VanillaOverwrite {
  /// Version of the document layout.
  pub schema_version: u32,
  /// The vanilla file these differences were written against.
  pub base: OverwriteFingerprint,
  /// The differences from that vanilla file, in the shape of a diff document's `diffs`.
  pub diffs: YmapDiffFields,
}

impl VanillaOverwrite {
  /// The current [`Self::schema_version`].
  pub const SCHEMA_VERSION: u32 = 2;

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
    if self.base != *vanilla {
      tracing::debug!(
        "Vanilla overwrite of {label} was written against another vanilla file; not applied"
      );
      return None;
    }
    Some(&self.diffs)
  }
}

/// Reads the YMAP overwrites under `<directory>/vanilla`, keyed by the lowercase path in the
/// vanilla archive of the file each was written against, such as `0000-base/ymap/map.ymap`.
///
/// Other file types are reported and ignored.
pub(super) fn load(directory: &Path) -> Result<BTreeMap<String, VanillaOverwrite>> {
  let vanilla_dir = directory.join("vanilla");
  let mut overwrites = BTreeMap::new();
  if !fs::is_dir(&vanilla_dir) {
    return Ok(overwrites);
  }
  for version in fs::read_dir(&vanilla_dir)? {
    let version_dir = version?.path();
    if !fs::is_dir(&version_dir) {
      tracing::warn!("Not a vanilla version directory; ignoring {}", version_dir.display());
      continue;
    }
    let version = utf8_name(&version_dir)?.to_ascii_lowercase();
    for entry in fs::read_dir(&version_dir)? {
      let path = entry?.path();
      if path.file_name().is_none_or(|name| name != "ymap") {
        tracing::warn!("Only YMAP vanilla overwrites are supported; ignoring {}", path.display());
      }
    }
    let ymap_dir = version_dir.join("ymap");
    if !fs::is_dir(&ymap_dir) {
      continue;
    }
    for entry in fs::read_dir(&ymap_dir)? {
      let path = entry?.path();
      let name = utf8_name(&path)?.to_ascii_lowercase();
      let Some(stream_name) = name.strip_suffix(DOCUMENT_EXTENSION).filter(|name| {
        fs::is_file(&path)
          && Path::new(name).extension().is_some_and(|extension| extension == "ymap")
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
      overwrites.insert(format!("{version}/ymap/{stream_name}"), overwrite);
    }
  }
  tracing::info!(
    "Loaded vanilla overwrites of {} YMAP files from {}",
    overwrites.len(),
    vanilla_dir.display()
  );
  Ok(overwrites)
}

fn utf8_name(path: &Path) -> Result<&str> {
  path
    .file_name()
    .and_then(|name| name.to_str())
    .ok_or_else(|| format!("Overwrite file name is not UTF-8: {}", path.display()).into())
}
