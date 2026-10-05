use structdiff::{Difference, StructDiff};

/// Instanced data definition in YMAP
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct YmapInstancedData {
  pub imap_link: String,
  pub prop_instance_list: Vec<String>,
  pub grass_instance_list: Vec<GrassInstanceBatch>,
}

/// Grass instance batch definition
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
#[derive(serde::Serialize, serde::Deserialize)]
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
pub struct BoundingBox {
  pub min: Vector4,
  pub max: Vector4,
}

/// 3D vector
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Vector3 {
  pub x: f32,
  pub y: f32,
  pub z: f32,
}

/// 4D vector
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Vector4 {
  pub x: f32,
  pub y: f32,
  pub z: f32,
  pub w: f32,
}

/// Individual grass instance data
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct GrassInstance {
  pub position: Vec<f32>,
  pub normal_x: u32,
  pub normal_y: u32,
  pub color: Vec<u32>,
  pub scale: u32,
  pub ao: u32,
  pub pad: Vec<u32>,
}
