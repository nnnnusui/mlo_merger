//! Distant LOD light differences, the `#[diff(with)]` strategy of `YmapDistantLodLights::items`.
//!
//! Lights are paired by position rounded to `tolerance.ymap_lod_light_position_decimals`;
//! lights whose position cannot be parsed are never paired. Positions moved within the rounding
//! are reported when they differ by at least `tolerance.ymap`.

use struct_diff::ApplyError;

use super::{
  YmapItemDiff,
  field::{pair_by_keys, rounded_position_key},
};
use crate::{
  common::position::Position,
  core::{
    config::matching::MatchTolerances,
    format::ymap::model::{
      YmapDistantLodLight, ymap_distant_lod_lights::YmapDistantLodLightFieldDiff,
    },
  },
};

/// How one distant LOD light of a modded YMAP differs from vanilla.
pub type YmapDistantLodLightDiff = YmapItemDiff<YmapDistantLodLight, YmapDistantLodLightFieldDiff>;

/// Parses a distant light's `"x y z"` position.
pub fn parse_distant_position(light: &YmapDistantLodLight) -> Option<Position> {
  parse_position(&light.position)
}

fn parse_position(text: &str) -> Option<Position> {
  let mut axes = text.split_whitespace().map(str::parse::<f32>);
  let position = Position {
    x: axes.next()?.ok()?,
    y: axes.next()?.ok()?,
    z: axes.next()?.ok()?,
  };
  axes.next().is_none().then_some(position)
}

/// Differences of `modded` lights from `vanilla`; see the module docs for how they pair.
pub fn diff(
  vanilla: &[YmapDistantLodLight],
  modded: &[YmapDistantLodLight],
  tolerances: &MatchTolerances,
) -> Option<Vec<YmapDistantLodLightDiff>> {
  let decimals = tolerances.ymap_lod_light_position_decimals;
  let keys = |items: &[YmapDistantLodLight]| {
    items
      .iter()
      .map(|light| {
        parse_distant_position(light).map(|position| rounded_position_key(&position, decimals))
      })
      .collect()
  };
  let pairing = pair_by_keys(keys(vanilla), keys(modded));
  let diffs = YmapItemDiff::collect(pairing, vanilla, modded, tolerances);
  (!diffs.is_empty()).then_some(diffs)
}

pub fn apply(
  items: &mut Vec<YmapDistantLodLight>,
  diffs: &[YmapDistantLodLightDiff],
) -> Result<(), ApplyError> {
  YmapItemDiff::apply(items, diffs)
}

/// The `#[diff(with)]` strategy of `YmapDistantLodLight::position`, comparing the `"x y z"`
/// text as a [`Position`] whose axes ignore changes below the tolerance.
///
/// Changes are only taken between parseable positions, so they are ignored when applied to a
/// position that cannot be parsed.
pub mod position {
  use struct_diff::{ApplyError, Diff};

  use super::parse_position;
  use crate::common::position::PositionDiff;

  pub fn diff(
    before: &str,
    after: &str,
    tolerance: &f32,
  ) -> Option<PositionDiff> {
    parse_position(before)?.diff(&parse_position(after)?, tolerance)
  }

  /// Rewrites `target` as `"x y z"` with the changed axes.
  pub fn apply(
    target: &mut String,
    diff: &PositionDiff,
  ) -> Result<(), ApplyError> {
    if let Some(mut position) = parse_position(target) {
      position.apply(diff)?;
      *target = format!("{} {} {}", position.x, position.y, position.z);
    }
    Ok(())
  }
}
