//! Distant LOD light differences.
//!
//! Lights are paired by position rounded to `tolerance.ymap_lod_light_position_decimals`;
//! lights whose position cannot be parsed are never paired. Positions moved within the rounding
//! are reported when they differ by at least `tolerance.ymap`.

use serde::{Deserialize, Serialize};

use super::{
  PositionDiff, YmapItemDiff,
  field::{changed, pair_by_keys, rounded_position_key, set_changed},
};
use crate::{
  common::{Result, position::Position},
  core::{
    config::matching::MatchTolerances,
    format::ymap::model::{Ymap, YmapDistantLodLight, YmapDistantLodLights},
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
pub type YmapDistantLodLightDiff = YmapItemDiff<YmapDistantLodLight, YmapDistantLodLightFieldDiff>;

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
  let items = YmapItemDiff::collect(pairing, modded_items, |vanilla_index, modded_index| {
    YmapDistantLodLightFieldDiff::between(
      &vanilla_items[vanilla_index],
      &modded_items[modded_index],
      tolerances.ymap,
    )
  });
  let diff = YmapDistantLodLightsDiff {
    num_street_lights: changed(&before.num_street_lights, &after.num_street_lights),
    category: changed(&before.category, &after.category),
    items,
  };
  (diff != YmapDistantLodLightsDiff::default()).then_some(diff)
}

impl YmapDistantLodLightsDiff {
  /// Overwrites the changed block fields of `lights` and rebuilds its items.
  pub fn apply_to(
    &self,
    lights: &mut YmapDistantLodLights,
  ) -> Result<()> {
    let Self {
      num_street_lights,
      category,
      items,
    } = self;
    set_changed(&mut lights.num_street_lights, num_street_lights);
    set_changed(&mut lights.category, category);
    lights.items =
      YmapItemDiff::apply("distant_lod_lights", &lights.items, items, |item, changes| {
        changes.apply_to(item)
      })?;
    Ok(())
  }
}

impl YmapDistantLodLightFieldDiff {
  /// Overwrites the changed fields of `light`, rewriting its position as `"x y z"`.
  ///
  /// Position changes are only recorded between parseable positions, so they are ignored for a
  /// light whose position cannot be parsed.
  pub fn apply_to(
    &self,
    light: &mut YmapDistantLodLight,
  ) {
    let Self {
      position,
      rgbi,
    } = self;
    if let (Some(diff), Some(mut position)) = (position, parse_distant_position(light)) {
      diff.apply_to(&mut position);
      light.position = format!("{} {} {}", position.x, position.y, position.z);
    }
    set_changed(&mut light.rgbi, rgbi);
  }

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
