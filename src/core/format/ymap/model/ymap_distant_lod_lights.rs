use crate::{
  common::position::{Position, PositionDiff},
  core::{config::matching::MatchTolerances, format::ymap::compare},
};

/// Single distant LOD light entry in YMAP
#[derive(
  Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize, struct_diff::Diff,
)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[diff(
  name = YmapDistantLodLightFieldDiff,
  context = MatchTolerances,
  key = YmapDistantLodLight::pairing_key,
  attr(cfg_attr(feature = "typescript", derive(specta::Type)))
)]
pub struct YmapDistantLodLight {
  /// `"x y z"`, compared as a position.
  #[diff(with = compare::distant_lod_lights::position, context = ctx.ymap, output = PositionDiff)]
  pub position: String,
  pub rgbi: String,
}

impl YmapDistantLodLight {
  /// Identifies a light by its position rounded to `tolerance.ymap_lod_light_position_decimals`
  /// decimal places; `None` when the position cannot be parsed.
  pub fn pairing_key(
    &self,
    tolerances: &MatchTolerances,
  ) -> Option<(i64, i64, i64)> {
    Some(
      Self::parse_position(&self.position)?
        .rounded_key(tolerances.ymap_lod_light_position_decimals),
    )
  }

  /// Parses an `"x y z"` position.
  pub fn parse_position(text: &str) -> Option<Position> {
    let mut axes = text.split_whitespace().map(str::parse::<f32>);
    let position = Position {
      x: axes.next()?.ok()?,
      y: axes.next()?.ok()?,
      z: axes.next()?.ok()?,
    };
    axes.next().is_none().then_some(position)
  }
}

/// Distant LOD lights structure of array in YMAP
#[derive(
  Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize, struct_diff::Diff,
)]
#[diff(
  context = MatchTolerances,
  attr(cfg_attr(feature = "typescript", derive(specta::Type)))
)]
pub struct YmapDistantLodLights {
  #[diff(nested)]
  pub items: Vec<YmapDistantLodLight>,
  pub num_street_lights: u32,
  pub category: u32,
}
