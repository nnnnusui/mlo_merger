//! Car generator differences, the `#[diff(with)]` strategy of `Ymap::car_generators`.
//!
//! Generators are paired in order when their positions match after rounding to
//! `tolerance.ymap_car_generator_position_decimals` decimal places; their fields are then
//! compared as `YmapCarGenerator` declares.

use struct_diff::ApplyError;

use super::{
  YmapItemDiff,
  field::{pair_by_keys, rounded_position_key},
};
use crate::core::{
  config::matching::MatchTolerances,
  format::ymap::model::{YmapCarGenerator, ymap_car_generator::YmapCarGeneratorFieldDiff},
};

/// How one car generator of a modded YMAP differs from vanilla.
pub type YmapCarGeneratorDiff = YmapItemDiff<YmapCarGenerator, YmapCarGeneratorFieldDiff>;

/// Differences of `modded` generators from `vanilla`; see the module docs for how they pair.
pub fn diff(
  vanilla: &[YmapCarGenerator],
  modded: &[YmapCarGenerator],
  tolerances: &MatchTolerances,
) -> Option<Vec<YmapCarGeneratorDiff>> {
  let decimals = tolerances.ymap_car_generator_position_decimals;
  let keys = |items: &[YmapCarGenerator]| {
    items.iter().map(|item| Some(rounded_position_key(&item.position, decimals))).collect()
  };
  let pairing = pair_by_keys(keys(vanilla), keys(modded));
  let diffs = YmapItemDiff::collect(pairing, vanilla, modded, tolerances);
  (!diffs.is_empty()).then_some(diffs)
}

pub fn apply(
  items: &mut Vec<YmapCarGenerator>,
  diffs: &[YmapCarGeneratorDiff],
) -> Result<(), ApplyError> {
  YmapItemDiff::apply(items, diffs)
}
