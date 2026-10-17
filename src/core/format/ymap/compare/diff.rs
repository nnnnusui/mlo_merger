//! JSON documents describing how a source YMAP differs from vanilla.
//!
//! The differences themselves are [`YmapDiffFields`], generated from `Ymap` by
//! `#[derive(Diff)]`: every field is optional and omitted when unchanged, so the documents list
//! only changes.

use serde::{Deserialize, Serialize};

use super::YmapDiffFields;

/// Differences of one source YMAP from the vanilla YMAP of the same name.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapComparisonDiff {
  /// Version of the document layout; documents of another version are regenerated.
  pub schema_version: u32,
  /// The source YMAP the differences were taken from.
  pub source: YmapComparisonFile,
  /// The vanilla YMAP used as the baseline.
  pub vanilla: YmapComparisonFile,
  pub diffs: YmapDiffFields,
}

impl YmapComparisonDiff {
  /// The current [`Self::schema_version`]. Documents without one predate versioning.
  pub const SCHEMA_VERSION: u32 = 4;
}

/// A compared YMAP file and its fingerprint when the diff was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapComparisonFile {
  pub path: String,
  pub sha256: String,
  pub size: u64,
  pub modified_seconds: u64,
  pub modified_nanos: u32,
}

/// The `#[diff(with)]` strategy of `Ymap::physics_dictionaries`.
///
/// Their order carries no meaning (CodeWalker only checks membership), so only the dictionaries
/// `modded` lists that `vanilla` does not are recorded, in `modded` order; removals are not
/// recorded, and applying keeps every dictionary already present.
pub mod physics_dictionaries {
  use struct_diff::ApplyError;

  pub fn diff<C: ?Sized>(
    vanilla: &[String],
    modded: &[String],
    _ctx: &C,
  ) -> Option<Vec<String>> {
    let added: Vec<String> =
      modded.iter().filter(|name| !vanilla.contains(name)).cloned().collect();
    (!added.is_empty()).then_some(added)
  }

  pub fn apply(
    dictionaries: &mut Vec<String>,
    added: &[String],
  ) -> Result<(), ApplyError> {
    for name in added {
      if !dictionaries.contains(name) {
        dictionaries.push(name.clone());
      }
    }
    Ok(())
  }
}
