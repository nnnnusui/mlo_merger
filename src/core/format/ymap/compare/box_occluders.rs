//! Box occluder differences, the `#[diff(with)]` strategy of `Ymap::box_occluders`.
//!
//! Box occluders are paired in order by center within `tolerance.ymap_box_occluder`, as
//! `YmapBoxOccluder::is_same` does; their fields are then compared as `YmapBoxOccluder`
//! declares.

use struct_diff::ApplyError;

use super::{YmapItemDiff, field::pair_in_order};
use crate::core::{
  config::matching::MatchTolerances,
  format::ymap::model::{YmapBoxOccluder, ymap_box_occluder::YmapBoxOccluderFieldDiff},
};

/// How one box occluder of a modded YMAP differs from vanilla.
pub type YmapBoxOccluderDiff = YmapItemDiff<YmapBoxOccluder, YmapBoxOccluderFieldDiff>;

/// Differences of `modded` box occluders from `vanilla`; see the module docs for how they pair.
pub fn diff(
  vanilla: &[YmapBoxOccluder],
  modded: &[YmapBoxOccluder],
  tolerances: &MatchTolerances,
) -> Option<Vec<YmapBoxOccluderDiff>> {
  let pairing = pair_in_order(vanilla, modded, |a, b| {
    let tolerance = tolerances.ymap_box_occluder;
    (a.i_center_x - b.i_center_x).abs() < tolerance
      && (a.i_center_y - b.i_center_y).abs() < tolerance
      && (a.i_center_z - b.i_center_z).abs() < tolerance
  });
  let diffs = YmapItemDiff::collect(pairing, vanilla, modded, tolerances);
  (!diffs.is_empty()).then_some(diffs)
}

pub fn apply(
  items: &mut Vec<YmapBoxOccluder>,
  diffs: &[YmapBoxOccluderDiff],
) -> Result<(), ApplyError> {
  YmapItemDiff::apply(items, diffs)
}
