//! Car generator differences.
//!
//! Generators are paired in order when their positions match after rounding to
//! `tolerance.ymap_car_generator_position_decimals` decimal places; float fields ignore
//! changes below `tolerance.ymap`.

use serde::{Deserialize, Serialize};

use super::{
  PositionDiff,
  field::{changed, changed_f32, pair_by_keys, rounded_position_key},
};
use crate::core::{
  config::matching::MatchTolerances,
  format::ymap::model::{Ymap, YmapCarGenerator},
};

/// How one car generator of a modded YMAP differs from vanilla.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum YmapCarGeneratorDiff {
  /// Present only in the modded YMAP, at `index`.
  Added {
    index: usize,
    car_generator: YmapCarGenerator,
  },
  /// Present only in vanilla, at `index`.
  Removed {
    index: usize,
  },
  /// Paired with a modded item and differing beyond the tolerances.
  Modified {
    vanilla_index: usize,
    modded_index: usize,
    changes: YmapCarGeneratorFieldDiff,
  },
}

/// Changed car generator fields, each holding its new value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapCarGeneratorFieldDiff {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub position: Option<PositionDiff>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub orient_x: Option<f32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub orient_y: Option<f32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub perpendicular_length: Option<f32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub car_model: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub flags: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub body_color_remap_1: Option<i32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub body_color_remap_2: Option<i32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub body_color_remap_3: Option<i32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub body_color_remap_4: Option<i32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub pop_group: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub livery: Option<i32>,
}

/// Collects `car_generators` differences; see the module docs for how items are paired.
pub fn car_generator_diffs(
  vanilla: &Ymap,
  modded: &Ymap,
  tolerances: &MatchTolerances,
) -> Vec<YmapCarGeneratorDiff> {
  let (vanilla_items, modded_items) = (&vanilla.car_generators, &modded.car_generators);
  let decimals = tolerances.ymap_car_generator_position_decimals;
  let keys = |items: &[YmapCarGenerator]| {
    items.iter().map(|item| Some(rounded_position_key(&item.position, decimals))).collect()
  };
  let pairing = pair_by_keys(keys(vanilla_items), keys(modded_items));
  let mut diffs = Vec::new();
  for (vanilla_index, modded_index) in pairing.pairs {
    let changes = YmapCarGeneratorFieldDiff::between(
      &vanilla_items[vanilla_index],
      &modded_items[modded_index],
      tolerances.ymap,
    );
    if changes != YmapCarGeneratorFieldDiff::default() {
      diffs.push(YmapCarGeneratorDiff::Modified {
        vanilla_index,
        modded_index,
        changes,
      });
    }
  }
  diffs.extend(pairing.removed.into_iter().map(|index| YmapCarGeneratorDiff::Removed {
    index,
  }));
  diffs.extend(pairing.added.into_iter().map(|index| YmapCarGeneratorDiff::Added {
    index,
    car_generator: modded_items[index].clone(),
  }));
  diffs
}

impl YmapCarGeneratorFieldDiff {
  /// Fields of `after` changed from `before`, ignoring float changes below `tolerance`.
  pub fn between(
    before: &YmapCarGenerator,
    after: &YmapCarGenerator,
    tolerance: f32,
  ) -> Self {
    Self {
      position: PositionDiff::between(&before.position, &after.position, tolerance),
      orient_x: changed_f32(before.orient_x, after.orient_x, tolerance),
      orient_y: changed_f32(before.orient_y, after.orient_y, tolerance),
      perpendicular_length: changed_f32(
        before.perpendicular_length,
        after.perpendicular_length,
        tolerance,
      ),
      car_model: changed(&before.car_model, &after.car_model),
      flags: changed(&before.flags, &after.flags),
      body_color_remap_1: changed(&before.body_color_remap_1, &after.body_color_remap_1),
      body_color_remap_2: changed(&before.body_color_remap_2, &after.body_color_remap_2),
      body_color_remap_3: changed(&before.body_color_remap_3, &after.body_color_remap_3),
      body_color_remap_4: changed(&before.body_color_remap_4, &after.body_color_remap_4),
      pop_group: changed(&before.pop_group, &after.pop_group),
      livery: changed(&before.livery, &after.livery),
    }
  }
}
