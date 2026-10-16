//! Box occluder differences.
//!
//! Box occluders are paired in order by center within `tolerance.ymap_box_occluder`, as
//! `YmapBoxOccluder::is_same` does; `i_cos_z`/`i_sin_z` changes up to
//! `tolerance.ymap_box_occluder_rotation` are ignored.

use serde::{Deserialize, Serialize};

use super::field::{changed, pair_in_order};
use crate::core::{
  config::matching::MatchTolerances,
  format::ymap::model::{Ymap, YmapBoxOccluder},
};

/// How one box occluder of a modded YMAP differs from vanilla.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum YmapBoxOccluderDiff {
  /// Present only in the modded YMAP, at `index`.
  Added {
    index: usize,
    box_occluder: YmapBoxOccluder,
  },
  /// Present only in vanilla, at `index`.
  Removed {
    index: usize,
  },
  /// Paired with a modded item and differing beyond the tolerances.
  Modified {
    vanilla_index: usize,
    modded_index: usize,
    changes: YmapBoxOccluderFieldDiff,
  },
}

/// Changed box occluder fields, each holding its new value, in stored units.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapBoxOccluderFieldDiff {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub i_center_x: Option<i32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub i_center_y: Option<i32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub i_center_z: Option<i32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub i_cos_z: Option<i32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub i_length: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub i_width: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub i_height: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub i_sin_z: Option<i32>,
}

/// Collects `box_occluders` differences; see the module docs for how items are paired.
pub fn box_occluder_diffs(
  vanilla: &Ymap,
  modded: &Ymap,
  tolerances: &MatchTolerances,
) -> Vec<YmapBoxOccluderDiff> {
  let (vanilla_items, modded_items) = (&vanilla.box_occluders, &modded.box_occluders);
  let pairing = pair_in_order(vanilla_items, modded_items, |a, b| {
    let tolerance = tolerances.ymap_box_occluder;
    (a.i_center_x - b.i_center_x).abs() < tolerance
      && (a.i_center_y - b.i_center_y).abs() < tolerance
      && (a.i_center_z - b.i_center_z).abs() < tolerance
  });
  let mut diffs = Vec::new();
  for (vanilla_index, modded_index) in pairing.pairs {
    let changes = YmapBoxOccluderFieldDiff::between(
      &vanilla_items[vanilla_index],
      &modded_items[modded_index],
      tolerances.ymap_box_occluder_rotation,
    );
    if changes != YmapBoxOccluderFieldDiff::default() {
      diffs.push(YmapBoxOccluderDiff::Modified {
        vanilla_index,
        modded_index,
        changes,
      });
    }
  }
  diffs.extend(pairing.removed.into_iter().map(|index| YmapBoxOccluderDiff::Removed {
    index,
  }));
  diffs.extend(pairing.added.into_iter().map(|index| YmapBoxOccluderDiff::Added {
    index,
    box_occluder: modded_items[index].clone(),
  }));
  diffs
}

impl YmapBoxOccluderFieldDiff {
  /// Fields of `after` changed from `before`, ignoring rotation changes up to `rotation`.
  pub fn between(
    before: &YmapBoxOccluder,
    after: &YmapBoxOccluder,
    rotation: i32,
  ) -> Self {
    let rotated = |from: i32, to: i32| ((from - to).abs() > rotation).then_some(to);
    Self {
      i_center_x: changed(&before.i_center_x, &after.i_center_x),
      i_center_y: changed(&before.i_center_y, &after.i_center_y),
      i_center_z: changed(&before.i_center_z, &after.i_center_z),
      i_cos_z: rotated(before.i_cos_z, after.i_cos_z),
      i_length: changed(&before.i_length, &after.i_length),
      i_width: changed(&before.i_width, &after.i_width),
      i_height: changed(&before.i_height, &after.i_height),
      i_sin_z: rotated(before.i_sin_z, after.i_sin_z),
    }
  }
}
