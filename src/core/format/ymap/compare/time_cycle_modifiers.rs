//! Time cycle modifier differences, the `#[diff(with)]` strategy of
//! `Ymap::time_cycle_modifiers`.
//!
//! Modifiers are paired in order when their minimum or maximum extents are equal, as the
//! merge diff does; their fields are then compared as `YmapTimeCycleModifier` declares.

use struct_diff::ApplyError;

use super::{YmapItemDiff, field::pair_in_order};
use crate::core::{
  config::matching::MatchTolerances,
  format::ymap::model::{
    YmapTimeCycleModifier, ymap_time_cycle_modifier::YmapTimeCycleModifierFieldDiff,
  },
};

/// How one time cycle modifier of a modded YMAP differs from vanilla.
pub type YmapTimeCycleModifierDiff =
  YmapItemDiff<YmapTimeCycleModifier, YmapTimeCycleModifierFieldDiff>;

/// Differences of `modded` modifiers from `vanilla`; see the module docs for how they pair.
pub fn diff(
  vanilla: &[YmapTimeCycleModifier],
  modded: &[YmapTimeCycleModifier],
  tolerances: &MatchTolerances,
) -> Option<Vec<YmapTimeCycleModifierDiff>> {
  let pairing = pair_in_order(vanilla, modded, |a, b| {
    a.min_extents == b.min_extents || a.max_extents == b.max_extents
  });
  let diffs = YmapItemDiff::collect(pairing, vanilla, modded, tolerances);
  (!diffs.is_empty()).then_some(diffs)
}

pub fn apply(
  items: &mut Vec<YmapTimeCycleModifier>,
  diffs: &[YmapTimeCycleModifierDiff],
) -> Result<(), ApplyError> {
  YmapItemDiff::apply(items, diffs)
}
