//! Occlude model differences.
//!
//! Models are paired in order when their minimum or maximum corners match in XY within
//! `tolerance.ymap_occlude_model`, as `YmapOccludeModel::is_same` does. Triangles are compared
//! exactly as a multiset: removed ones are listed by their vanilla index, added ones in full.

use serde::{Deserialize, Serialize};

use super::{
  PositionDiff, YmapItemDiff,
  field::{changed, pair_in_order, set_changed},
  item_diff::{ItemChange, apply_changes},
};
use crate::{
  common::{Result, position::Position, triangle::Triangle},
  core::{
    config::matching::MatchTolerances,
    format::ymap::model::{Ymap, YmapOccludeModel},
  },
};

/// How one occlude model of a modded YMAP differs from vanilla.
pub type YmapOccludeModelDiff = YmapItemDiff<YmapOccludeModel, YmapOccludeModelFieldDiff>;

/// Changed occlude model fields, each holding its new value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapOccludeModelFieldDiff {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub bmin: Option<PositionDiff>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub bmax: Option<PositionDiff>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub flags: Option<u32>,
  /// Indices into the vanilla model's triangles that the modded model lacks.
  #[serde(skip_serializing_if = "Vec::is_empty", default)]
  pub removed_triangles: Vec<usize>,
  /// Triangles only the modded model has.
  #[serde(skip_serializing_if = "Vec::is_empty", default)]
  pub added_triangles: Vec<Triangle>,
}

/// Collects `occlude_models` differences; see the module docs for how items are paired.
pub fn occlude_model_diffs(
  vanilla: &Ymap,
  modded: &Ymap,
  tolerances: &MatchTolerances,
) -> Vec<YmapOccludeModelDiff> {
  let (vanilla_items, modded_items) = (&vanilla.occlude_models, &modded.occlude_models);
  let pairing = pair_in_order(vanilla_items, modded_items, |a, b| {
    let tolerance = tolerances.ymap_occlude_model;
    let close =
      |p: &Position, q: &Position| (p.x - q.x).abs() < tolerance && (p.y - q.y).abs() < tolerance;
    close(&a.bmin, &b.bmin) || close(&a.bmax, &b.bmax)
  });
  YmapItemDiff::collect(pairing, modded_items, |vanilla_index, modded_index| {
    YmapOccludeModelFieldDiff::between(&vanilla_items[vanilla_index], &modded_items[modded_index])
  })
}

impl YmapOccludeModelFieldDiff {
  /// Overwrites the changed fields of `item`, removing its listed triangles and appending the
  /// added ones.
  pub fn apply_to(
    &self,
    item: &mut YmapOccludeModel,
  ) -> Result<()> {
    let Self {
      bmin,
      bmax,
      flags,
      removed_triangles,
      added_triangles,
    } = self;
    if let Some(corner) = bmin {
      corner.apply_to(&mut item.bmin);
    }
    if let Some(corner) = bmax {
      corner.apply_to(&mut item.bmax);
    }
    set_changed(&mut item.flags, flags);
    let kept = item.triangles.len().saturating_sub(removed_triangles.len());
    let changes = removed_triangles
      .iter()
      .map(|&index| ItemChange::Removed {
        index,
      })
      .chain(added_triangles.iter().enumerate().map(|(offset, item)| ItemChange::Added {
        index: kept + offset,
        item,
      }));
    item.triangles =
      apply_changes("occlude model triangles", &item.triangles, changes, |_, _: &()| Ok(()))?;
    Ok(())
  }

  /// Fields and triangles of `after` changed from `before`, compared exactly.
  pub fn between(
    before: &YmapOccludeModel,
    after: &YmapOccludeModel,
  ) -> Self {
    let triangles = pair_in_order(&before.triangles, &after.triangles, |a, b| a == b);
    Self {
      bmin: PositionDiff::between(&before.bmin, &after.bmin, f32::MIN_POSITIVE),
      bmax: PositionDiff::between(&before.bmax, &after.bmax, f32::MIN_POSITIVE),
      flags: changed(&before.flags, &after.flags),
      removed_triangles: triangles.removed,
      added_triangles: triangles
        .added
        .into_iter()
        .map(|index| after.triangles[index].clone())
        .collect(),
    }
  }
}
