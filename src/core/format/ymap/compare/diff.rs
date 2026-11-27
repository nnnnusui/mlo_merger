//! JSON documents describing how a source YMAP differs from vanilla.
//!
//! The differences themselves are [`YmapDiffFields`], generated from `Ymap` by
//! `#[derive(Diff)]`: every field is optional and omitted when unchanged, so the documents list
//! only changes.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::YmapDiffFields;
use crate::common::Result;
use crate::core::config::matching::MatchTolerances;
use crate::core::format::compared::{ComparedFile, PropagationOrigin};

/// Differences of one source YMAP from the vanilla YMAP of the same name.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapComparisonDiff {
  /// Version of the document layout; documents of another version are regenerated.
  pub schema_version: u32,
  /// The YMAP the differences lead to: the source YMAP.
  pub target: ComparedFile,
  /// The YMAP the differences are taken from: the vanilla YMAP of the same name, the empty
  /// YMAP for one no vanilla YMAP is named after, or the base source's YMAP.
  pub base: ComparedFile,
  /// The matching tolerances the differences were taken with.
  pub tolerances: MatchTolerances,
  /// For one YMAP of a LOD light pair, the other one, whose removals `diffs` also makes as
  /// implied removals; see [`super::lod_light_pairs`].
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub partner: Option<YmapComparisonPartner>,
  /// A digest of the vanilla versions of the YMAP the source was compared with to find its
  /// baseline, when there are several.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub vanilla_versions: Option<String>,
  /// The vanilla version the source differs least from, taken as the one it was made from,
  /// when it is not `base`: `diffs` leave out what the source keeps from it (see
  /// [`super::baseline`]).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub baseline: Option<ComparedFile>,
  pub diffs: YmapDiffFields,
}

/// The other YMAP of a LOD light pair and its vanilla baseline, when the diff was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapComparisonPartner {
  /// The source's own YMAP of that name, or the vanilla one when the source has none.
  pub target: ComparedFile,
  pub base: ComparedFile,
  /// A digest of the vanilla versions of the other YMAP its baseline was found among, when
  /// there are several, as its removals are taken relative to that baseline.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub vanilla_versions: Option<String>,
}

impl YmapComparisonDiff {
  /// The current [`Self::schema_version`]. Documents without one predate versioning.
  pub const SCHEMA_VERSION: u32 = 16;

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
      && document.target.describes(source)?
      && document.base.describes(vanilla)?;
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
  pub base: ComparedFile,
  /// The resource's YMAPs the changes are carried over from.
  pub origins: Vec<PropagationOrigin>,
  pub diffs: YmapDiffFields,
}

impl YmapPropagatedDiff {
  /// The current [`Self::schema_version`].
  pub const SCHEMA_VERSION: u32 = 3;

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
      document.schema_version == Self::SCHEMA_VERSION && document.base.describes(vanilla)?;
    Ok(current.then_some(document))
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
