//! Occlude model differences, the `#[diff(with)]` strategy of `Ymap::occlude_models`.
//!
//! Models are paired in order when their minimum or maximum corners match in XY within
//! `tolerance.ymap_occlude_model`, as `YmapOccludeModel::is_same` does; their fields are then
//! compared as `YmapOccludeModel` declares.

use struct_diff::ApplyError;

use super::{YmapItemDiff, field::pair_in_order};
use crate::{
  common::position::Position,
  core::{
    config::matching::MatchTolerances,
    format::ymap::model::{YmapOccludeModel, ymap_occlude_model::YmapOccludeModelFieldDiff},
  },
};

/// How one occlude model of a modded YMAP differs from vanilla.
pub type YmapOccludeModelDiff = YmapItemDiff<YmapOccludeModel, YmapOccludeModelFieldDiff>;

/// Differences of `modded` models from `vanilla`; see the module docs for how they pair.
pub fn diff(
  vanilla: &[YmapOccludeModel],
  modded: &[YmapOccludeModel],
  tolerances: &MatchTolerances,
) -> Option<Vec<YmapOccludeModelDiff>> {
  let tolerance = tolerances.ymap_occlude_model;
  let close =
    |p: &Position, q: &Position| (p.x - q.x).abs() < tolerance && (p.y - q.y).abs() < tolerance;
  let pairing =
    pair_in_order(vanilla, modded, |a, b| close(&a.bmin, &b.bmin) || close(&a.bmax, &b.bmax));
  let diffs = YmapItemDiff::collect(pairing, vanilla, modded, &());
  (!diffs.is_empty()).then_some(diffs)
}

pub fn apply(
  items: &mut Vec<YmapOccludeModel>,
  diffs: &[YmapOccludeModelDiff],
) -> Result<(), ApplyError> {
  YmapItemDiff::apply(items, diffs)
}

/// The `#[diff(with)]` strategy of `YmapOccludeModel::triangles`.
///
/// Triangles are compared exactly as a multiset: removed ones are listed by their vanilla index,
/// added ones in full at their modded index.
pub mod triangles {
  use struct_diff::ApplyError;

  use super::super::{NoFieldDiff, YmapItemDiff, field::pair_in_order};
  use crate::common::triangle::Triangle;

  /// How one triangle of a modded occlude model differs from vanilla.
  pub type YmapTriangleDiff = YmapItemDiff<Triangle, NoFieldDiff>;

  /// Triangles only one of `vanilla` and `modded` has.
  pub fn diff<C: ?Sized>(
    vanilla: &[Triangle],
    modded: &[Triangle],
    _ctx: &C,
  ) -> Option<Vec<YmapTriangleDiff>> {
    let diffs = YmapItemDiff::unpaired(pair_in_order(vanilla, modded, |a, b| a == b), modded);
    (!diffs.is_empty()).then_some(diffs)
  }

  pub fn apply(
    items: &mut Vec<Triangle>,
    diffs: &[YmapTriangleDiff],
  ) -> Result<(), ApplyError> {
    YmapItemDiff::apply_unpaired(items, diffs)
  }
}
