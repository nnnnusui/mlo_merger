//! Time cycle modifier differences.
//!
//! Modifiers are paired in order when their minimum or maximum extents are equal, as the
//! merge diff does; float fields ignore changes below `tolerance.ymap`.

use serde::{Deserialize, Serialize};

use super::{
  PositionDiff,
  field::{changed, changed_f32, pair_in_order},
};
use crate::core::{
  config::matching::MatchTolerances,
  format::ymap::model::{Ymap, YmapTimeCycleModifier},
};

/// How one time cycle modifier of a modded YMAP differs from vanilla.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum YmapTimeCycleModifierDiff {
  /// Present only in the modded YMAP, at `index`.
  Added {
    index: usize,
    time_cycle_modifier: YmapTimeCycleModifier,
  },
  /// Present only in vanilla, at `index`.
  Removed {
    index: usize,
  },
  /// Paired with a modded item and differing beyond the tolerances.
  Modified {
    vanilla_index: usize,
    modded_index: usize,
    changes: YmapTimeCycleModifierFieldDiff,
  },
}

/// Changed time cycle modifier fields, each holding its new value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapTimeCycleModifierFieldDiff {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub min_extents: Option<PositionDiff>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub max_extents: Option<PositionDiff>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub percentage: Option<f32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub range: Option<f32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub start_hour: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub end_hour: Option<u32>,
}

/// Collects `time_cycle_modifiers` differences; see the module docs for how items are paired.
pub fn time_cycle_modifier_diffs(
  vanilla: &Ymap,
  modded: &Ymap,
  tolerances: &MatchTolerances,
) -> Vec<YmapTimeCycleModifierDiff> {
  let (vanilla_items, modded_items) = (&vanilla.time_cycle_modifiers, &modded.time_cycle_modifiers);
  let pairing = pair_in_order(vanilla_items, modded_items, |a, b| {
    a.min_extents == b.min_extents || a.max_extents == b.max_extents
  });
  let mut diffs = Vec::new();
  for (vanilla_index, modded_index) in pairing.pairs {
    let changes = YmapTimeCycleModifierFieldDiff::between(
      &vanilla_items[vanilla_index],
      &modded_items[modded_index],
      tolerances.ymap,
    );
    if changes != YmapTimeCycleModifierFieldDiff::default() {
      diffs.push(YmapTimeCycleModifierDiff::Modified {
        vanilla_index,
        modded_index,
        changes,
      });
    }
  }
  diffs.extend(pairing.removed.into_iter().map(|index| YmapTimeCycleModifierDiff::Removed {
    index,
  }));
  diffs.extend(pairing.added.into_iter().map(|index| YmapTimeCycleModifierDiff::Added {
    index,
    time_cycle_modifier: modded_items[index].clone(),
  }));
  diffs
}

impl YmapTimeCycleModifierFieldDiff {
  /// Fields of `after` changed from `before`, ignoring float changes below `tolerance`.
  pub fn between(
    before: &YmapTimeCycleModifier,
    after: &YmapTimeCycleModifier,
    tolerance: f32,
  ) -> Self {
    Self {
      name: changed(&before.name, &after.name),
      min_extents: PositionDiff::between(&before.min_extents, &after.min_extents, tolerance),
      max_extents: PositionDiff::between(&before.max_extents, &after.max_extents, tolerance),
      percentage: changed_f32(before.percentage, after.percentage, tolerance),
      range: changed_f32(before.range, after.range, tolerance),
      start_hour: changed(&before.start_hour, &after.start_hour),
      end_hour: changed(&before.end_hour, &after.end_hour),
    }
  }
}
