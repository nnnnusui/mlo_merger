//! JSON documents describing how a source YMAP differs from vanilla.
//!
//! The differences themselves are [`YmapDiffFields`], generated from `Ymap` by
//! `#[derive(Diff)]`: every field is optional and omitted when unchanged, so the documents list
//! only changes.

use crate::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::YmapDiffFields;
use crate::common::{Result, fingerprint::Fingerprint};
use crate::core::config::matching::MatchTolerances;

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
  /// The matching tolerances the differences were taken with.
  pub tolerances: MatchTolerances,
  /// For one YMAP of a LOD light pair, the other one, whose removals `diffs` also makes as
  /// implied removals; see [`super::lod_light_pairs`].
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub partner: Option<YmapComparisonPartner>,
  pub diffs: YmapDiffFields,
}

/// The other YMAP of a LOD light pair and its vanilla baseline, when the diff was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapComparisonPartner {
  /// The source's own YMAP of that name, or the vanilla one when the source has none.
  pub source: YmapComparisonFile,
  pub vanilla: YmapComparisonFile,
}

impl YmapComparisonDiff {
  /// The current [`Self::schema_version`]. Documents without one predate versioning.
  pub const SCHEMA_VERSION: u32 = 11;

  /// Reads the document at `path` when it holds the differences of `source` from `vanilla` as
  /// both are now, taken with `tolerances`; `None` when it is missing, of another layout or
  /// stale.
  ///
  /// The [`Self::partner`] files are not checked; the source cache keeps them current.
  /// Files whose size and modification time match the document are not hashed again.
  pub fn read_current(
    path: &Path,
    source: &Path,
    vanilla: &Path,
    tolerances: &MatchTolerances,
  ) -> Result<Option<Self>> {
    let Ok(document) = crate::common::json::read_json::<Self>(path) else {
      return Ok(None);
    };
    let current = document.schema_version == Self::SCHEMA_VERSION
      && document.tolerances == *tolerances
      && document.source.describes(source)?
      && document.vanilla.describes(vanilla)?;
    Ok(current.then_some(document))
  }
}

/// Changes a resource makes to items of vanilla YMAPs, carried over to the same items of a newer
/// generation of those YMAPs, which the resource does not ship.
///
/// DLCs ship vanilla YMAPs again under prefixes of their own, and the game loads every
/// generation, so an item a resource removes or modifies in one generation is removed or
/// modified in the newer ones too. Each carried change records in its `reason` the item it was
/// carried over from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapPropagatedDiff {
  /// Version of the document layout; documents of another version are regenerated.
  pub schema_version: u32,
  /// The resource making the changes, by its ID, its path relative to the source root.
  pub resource: String,
  /// The vanilla YMAP the changes are carried to, as it was when they were.
  pub vanilla: YmapComparisonFile,
  /// The resource's YMAPs the changes are carried over from.
  pub origins: Vec<YmapPropagationOrigin>,
  pub diffs: YmapDiffFields,
}

/// A YMAP of a resource that changes are carried over from, and its vanilla baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapPropagationOrigin {
  pub source: YmapComparisonFile,
  pub vanilla: YmapComparisonFile,
}

impl YmapPropagatedDiff {
  /// The current [`Self::schema_version`].
  pub const SCHEMA_VERSION: u32 = 1;

  /// Reads the document at `path` when it is of the current layout and was written against
  /// `vanilla` as it is now; `None` otherwise.
  ///
  /// The origins are not checked; the source cache keeps them current.
  pub fn read_current(
    path: &Path,
    vanilla: &Path,
  ) -> Result<Option<Self>> {
    let Ok(document) = crate::common::json::read_json::<Self>(path) else {
      return Ok(None);
    };
    let current =
      document.schema_version == Self::SCHEMA_VERSION && document.vanilla.describes(vanilla)?;
    Ok(current.then_some(document))
  }
}

/// A compared YMAP file and its fingerprint when the diff was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapComparisonFile {
  /// The file relative to the store holding it: the source cache, the vanilla cache, or the
  /// source root for a source file that was not moved into the cache.
  pub path: String,
  pub sha256: String,
  pub size: u64,
  pub modified_seconds: u64,
  pub modified_nanos: u32,
}

impl YmapComparisonFile {
  /// Whether the file at `path` (after resolving links) has the recorded contents.
  ///
  /// The document names the file, so where it is does not matter: a file whose size and
  /// modification time match is not hashed again.
  pub fn describes(
    &self,
    path: &Path,
  ) -> Result<bool> {
    let path = fs::canonicalize(path)?;
    let previous = Fingerprint {
      sha256: self.sha256.clone(),
      size: self.size,
      modified_seconds: self.modified_seconds,
      modified_nanos: self.modified_nanos,
    };
    let current = Fingerprint::read(&path, Some(&previous), false)?;
    Ok(current.sha256 == self.sha256 && current.size == self.size)
  }

  /// Whether `other` has the same contents, wherever it is and whenever it was modified.
  pub fn same_content(
    &self,
    other: &Self,
  ) -> bool {
    self.sha256 == other.sha256 && self.size == other.size
  }
}

/// The `#[diff(with)]` strategy of `Ymap::physics_dictionaries`, comparing names by hash as a
/// multiset, as prop instance names are.
///
/// CodeWalker only checks membership, so merging adds dictionaries but never removes them; see
/// [`super::merge::IgnoredChange::PhysicsDictionaryRemovals`].
pub mod physics_dictionaries {
  pub use super::super::instanced_data::prop_instances::{apply, diff};
}
