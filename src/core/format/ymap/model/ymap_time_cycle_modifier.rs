use crate::{common::position::Position, core::config::matching::MatchTolerances};

/// Time cycle modifier definition in YMAP
#[derive(Debug, Clone, PartialEq, struct_diff::Diff, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[diff(
  name = YmapTimeCycleModifierFieldDiff,
  context = MatchTolerances,
  attr(cfg_attr(feature = "typescript", derive(specta::Type)))
)]
pub struct YmapTimeCycleModifier {
  pub name: String,
  #[diff(nested, context = ctx.ymap)]
  pub min_extents: Position,
  #[diff(nested, context = ctx.ymap)]
  pub max_extents: Position,
  #[diff(threshold = ctx.ymap)]
  pub percentage: f32,
  #[diff(threshold = ctx.ymap)]
  pub range: f32,
  pub start_hour: u32,
  pub end_hour: u32,
}
