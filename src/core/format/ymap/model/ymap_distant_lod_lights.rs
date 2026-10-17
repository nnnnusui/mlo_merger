use crate::{
  common::position::PositionDiff,
  core::{
    config::matching::MatchTolerances,
    format::ymap::compare::{self, YmapDistantLodLightDiff},
  },
};

/// Single distant LOD light entry in YMAP
#[derive(
  Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize, struct_diff::Diff,
)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[diff(
  name = YmapDistantLodLightFieldDiff,
  context = MatchTolerances,
  attr(cfg_attr(feature = "typescript", derive(specta::Type)))
)]
pub struct YmapDistantLodLight {
  /// `"x y z"`, compared as a position.
  #[diff(with = compare::distant_lod_lights::position, context = ctx.ymap, output = PositionDiff)]
  pub position: String,
  pub rgbi: String,
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
  #[diff(with = compare::distant_lod_lights, output = Vec<YmapDistantLodLightDiff>)]
  pub items: Vec<YmapDistantLodLight>,
  pub num_street_lights: u32,
  pub category: u32,
}
