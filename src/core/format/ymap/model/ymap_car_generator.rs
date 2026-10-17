use crate::{common::position::Position, core::config::matching::MatchTolerances};

/// Car generator definition in YMAP
#[derive(Debug, Clone, PartialEq, struct_diff::Diff, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[diff(
  name = YmapCarGeneratorFieldDiff,
  context = MatchTolerances,
  key = YmapCarGenerator::pairing_key,
  attr(cfg_attr(feature = "typescript", derive(specta::Type)))
)]
pub struct YmapCarGenerator {
  #[diff(nested, context = ctx.ymap)]
  pub position: Position,
  #[diff(threshold = ctx.ymap)]
  pub orient_x: f32,
  #[diff(threshold = ctx.ymap)]
  pub orient_y: f32,
  #[diff(threshold = ctx.ymap)]
  pub perpendicular_length: f32,
  pub car_model: String,
  pub flags: u32,
  pub body_color_remap_1: i32,
  pub body_color_remap_2: i32,
  pub body_color_remap_3: i32,
  pub body_color_remap_4: i32,
  pub pop_group: String,
  pub livery: i32,
}

impl YmapCarGenerator {
  /// Identifies a generator by its position rounded to
  /// `tolerance.ymap_car_generator_position_decimals` decimal places.
  pub fn pairing_key(
    &self,
    tolerances: &MatchTolerances,
  ) -> Option<(i64, i64, i64)> {
    Some(self.position.rounded_key(tolerances.ymap_car_generator_position_decimals))
  }
}
