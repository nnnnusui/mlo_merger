//! Occlude model differences.
//!
//! Models are paired in order when their minimum or maximum corners match in XY within
//! `tolerance.ymap_occlude_model`, as `YmapOccludeModel::is_same` does. Triangles are compared
//! exactly as a multiset: removed ones are listed by their vanilla index, added ones in full.

use serde::{Deserialize, Serialize};

use super::{
  PositionDiff,
  field::{changed, pair_in_order},
};
use crate::{
  common::{position::Position, triangle::Triangle},
  core::{
    config::matching::MatchTolerances,
    format::ymap::model::{Ymap, YmapOccludeModel},
  },
};

/// How one occlude model of a modded YMAP differs from vanilla.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum YmapOccludeModelDiff {
  /// Present only in the modded YMAP, at `index`.
  Added {
    index: usize,
    occlude_model: YmapOccludeModel,
  },
  /// Present only in vanilla, at `index`.
  Removed {
    index: usize,
  },
  /// Paired with a modded item and differing beyond the tolerances.
  Modified {
    vanilla_index: usize,
    modded_index: usize,
    changes: YmapOccludeModelFieldDiff,
  },
}

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
  let mut diffs = Vec::new();
  for (vanilla_index, modded_index) in pairing.pairs {
    let changes = YmapOccludeModelFieldDiff::between(
      &vanilla_items[vanilla_index],
      &modded_items[modded_index],
    );
    if changes != YmapOccludeModelFieldDiff::default() {
      diffs.push(YmapOccludeModelDiff::Modified {
        vanilla_index,
        modded_index,
        changes,
      });
    }
  }
  diffs.extend(pairing.removed.into_iter().map(|index| YmapOccludeModelDiff::Removed {
    index,
  }));
  diffs.extend(pairing.added.into_iter().map(|index| YmapOccludeModelDiff::Added {
    index,
    occlude_model: modded_items[index].clone(),
  }));
  diffs
}

impl YmapOccludeModelFieldDiff {
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
