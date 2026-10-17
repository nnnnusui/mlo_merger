use structdiff::{Difference, StructDiff};

use crate::core::{
  config::matching::MatchTolerances,
  format::ymap::compare::{self, YmapGrassBatchDiff, YmapPropInstanceDiff},
};

/// Instanced data definition in YMAP
#[derive(Debug, Clone, PartialEq, Difference, struct_diff::Diff)]
#[difference(expose)]
#[derive(Default, serde::Serialize, serde::Deserialize)]
#[diff(
  context = MatchTolerances,
  attr(cfg_attr(feature = "typescript", derive(specta::Type)))
)]
pub struct YmapInstancedData {
  #[diff(with = compare::instanced_data::imap_link)]
  pub imap_link: String,
  #[diff(with = compare::instanced_data::prop_instances, output = Vec<YmapPropInstanceDiff>)]
  pub prop_instance_list: Vec<String>,
  #[diff(
    with = compare::instanced_data::grass_batches,
    context = ctx.ymap_grass_position,
    output = Vec<YmapGrassBatchDiff>
  )]
  pub grass_instance_list: Vec<GrassInstanceBatch>,
}

/// Grass instance batch definition
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct GrassInstanceBatch {
  pub batch_aabb: BoundingBox,
  pub scale_range: Vector3,
  pub archetype_name: String,
  pub lod_dist: f32,
  pub lod_fade_start_dist: f32,
  pub lod_inst_fade_range: f32,
  pub orient_to_terrain: u32,
  pub instances: Vec<GrassInstance>,
}

/// Bounding box with min and max vectors
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct BoundingBox {
  pub min: Vector4,
  pub max: Vector4,
}

/// 3D vector
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct Vector3 {
  pub x: f32,
  pub y: f32,
  pub z: f32,
}

/// 4D vector
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct Vector4 {
  pub x: f32,
  pub y: f32,
  pub z: f32,
  pub w: f32,
}

/// Individual grass instance data
#[derive(Debug, Clone, PartialEq, Difference, struct_diff::Diff)]
#[difference(expose)]
#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[diff(
  name = YmapGrassInstanceFieldDiff,
  attr(cfg_attr(feature = "typescript", derive(specta::Type)))
)]
pub struct GrassInstance {
  /// Packed relative to the batch bounds, so compared in world space by the batch.
  #[diff(skip)]
  pub position: Vec<f32>,
  pub normal_x: u32,
  pub normal_y: u32,
  pub color: Vec<u32>,
  pub scale: u32,
  pub ao: u32,
  /// Rewritten by re-exporting tools.
  #[diff(skip)]
  pub pad: Vec<u32>,
}
