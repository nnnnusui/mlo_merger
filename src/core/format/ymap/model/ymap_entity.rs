use super::ymap_extension::YmapExtension;
use crate::{
  common::{position::Position, rotation::Rotation},
  core::{config::matching::MatchTolerances, format::ymap::compare},
};

/// Entity definition in YMAP
#[derive(Debug, Clone, PartialEq, struct_diff::Diff, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[diff(
  name = YmapEntityFieldDiff,
  context = MatchTolerances,
  attr(cfg_attr(feature = "typescript", derive(specta::Type)))
)]
pub struct YmapEntity {
  pub entity_type: String,
  pub archetype_name: String,
  pub flags: u32,
  pub guid: u32,
  #[diff(nested, context = ctx.ymap)]
  pub position: Position,
  #[diff(nested, context = ctx.ymap)]
  pub rotation: Rotation,
  #[diff(threshold = ctx.ymap)]
  pub scale_x_y: f32,
  #[diff(threshold = ctx.ymap)]
  pub scale_z: f32,
  /// Compared and applied by `compare::entities` as the entity it points at.
  #[diff(skip)]
  pub parent_index: i32,
  #[diff(threshold = ctx.ymap)]
  pub lod_dist: f32,
  #[diff(threshold = ctx.ymap)]
  pub child_lod_dist: f32,
  pub lod_level: String,
  pub num_children: u32,
  pub priority_level: String,
  pub ambient_occlusion_multiplier: u8,
  pub artificial_ambient_occlusion: u8,
  pub tint_value: u32,
  /// Compared as a whole; a change records every extension the source has.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  #[diff(with = compare::extensions, context = ctx.ymap)]
  pub extensions: Vec<YmapExtension>,
}
