//! Distant LOD light differences.
//!
//! Lights are paired by position rounded to `tolerance.ymap_lod_light_position_decimals`;
//! lights whose position cannot be parsed are never paired. Positions moved within the rounding
//! are reported when they differ by at least `tolerance.ymap`.

use serde::{Deserialize, Serialize};

use super::{
  PositionDiff,
  field::{changed, pair_by_keys, rounded_position_key},
};
use crate::{
  common::position::Position,
  core::{
    config::matching::MatchTolerances,
    format::ymap::model::{Ymap, YmapDistantLodLight},
  },
};

/// Changed distant LOD light block fields and lights.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapDistantLodLightsDiff {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub num_street_lights: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub category: Option<u32>,
  #[serde(skip_serializing_if = "Vec::is_empty", default)]
  pub items: Vec<YmapDistantLodLightDiff>,
}

/// How one distant LOD light of a modded YMAP differs from vanilla.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum YmapDistantLodLightDiff {
  /// Present only in the modded YMAP, at `index`.
  Added {
    index: usize,
    distant_lod_light: YmapDistantLodLight,
  },
  /// Present only in vanilla, at `index`.
  Removed {
    index: usize,
  },
  /// Paired with a modded item and differing beyond the tolerances.
  Modified {
    vanilla_index: usize,
    modded_index: usize,
    changes: YmapDistantLodLightFieldDiff,
  },
}

/// Changed distant LOD light fields, each holding its new value.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapDistantLodLightFieldDiff {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub position: Option<PositionDiff>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub rgbi: Option<String>,
}

/// Parses a distant light's `"x y z"` position.
pub fn parse_distant_position(light: &YmapDistantLodLight) -> Option<Position> {
  let mut axes = light.position.split_whitespace().map(str::parse::<f32>);
  let position = Position {
    x: axes.next()?.ok()?,
    y: axes.next()?.ok()?,
    z: axes.next()?.ok()?,
  };
  axes.next().is_none().then_some(position)
}

/// Collects `distant_lod_lights` differences, or `None` when there are none.
pub fn distant_lod_lights_diff(
  vanilla: &Ymap,
  modded: &Ymap,
  tolerances: &MatchTolerances,
) -> Option<YmapDistantLodLightsDiff> {
  let (before, after) = (&vanilla.distant_lod_lights, &modded.distant_lod_lights);
  let (vanilla_items, modded_items) = (&before.items, &after.items);
  let decimals = tolerances.ymap_lod_light_position_decimals;
  let keys = |items: &[YmapDistantLodLight]| {
    items
      .iter()
      .map(|light| {
        parse_distant_position(light).map(|position| rounded_position_key(&position, decimals))
      })
      .collect()
  };
  let pairing = pair_by_keys(keys(vanilla_items), keys(modded_items));
  let items = {
    let mut diffs = Vec::new();
    for (vanilla_index, modded_index) in pairing.pairs {
      let changes = YmapDistantLodLightFieldDiff::between(
        &vanilla_items[vanilla_index],
        &modded_items[modded_index],
        tolerances.ymap,
      );
      if changes != YmapDistantLodLightFieldDiff::default() {
        diffs.push(YmapDistantLodLightDiff::Modified {
          vanilla_index,
          modded_index,
          changes,
        });
      }
    }
    diffs.extend(pairing.removed.into_iter().map(|index| YmapDistantLodLightDiff::Removed {
      index,
    }));
    diffs.extend(pairing.added.into_iter().map(|index| YmapDistantLodLightDiff::Added {
      index,
      distant_lod_light: modded_items[index].clone(),
    }));
    diffs
  };
  let diff = YmapDistantLodLightsDiff {
    num_street_lights: changed(&before.num_street_lights, &after.num_street_lights),
    category: changed(&before.category, &after.category),
    items,
  };
  (diff != YmapDistantLodLightsDiff::default()).then_some(diff)
}

impl YmapDistantLodLightFieldDiff {
  /// Fields of `after` changed from `before`, ignoring position changes below `tolerance`.
  pub fn between(
    before: &YmapDistantLodLight,
    after: &YmapDistantLodLight,
    tolerance: f32,
  ) -> Self {
    let position = match (parse_distant_position(before), parse_distant_position(after)) {
      (Some(from), Some(to)) => PositionDiff::between(&from, &to, tolerance),
      _ => None,
    };
    Self {
      position,
      rgbi: changed(&before.rgbi, &after.rgbi),
    }
  }
}
