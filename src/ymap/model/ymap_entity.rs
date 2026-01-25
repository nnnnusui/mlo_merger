use crate::common::{Position, Rotation};

/// Entity definition in YMAP
#[derive(Debug)]
pub struct YmapEntity {
  pub entity_type: String,
  pub archetype_name: String,
  pub flags: u32,
  pub guid: u32,
  pub position: Position,
  pub rotation: Rotation,
  pub scale_x_y: f32,
  pub scale_z: f32,
  pub parent_index: i32,
  pub lod_dist: f32,
  pub child_lod_dist: f32,
  pub lod_level: String,
  pub num_children: u32,
  pub priority_level: String,
  pub ambient_occlusion_multiplier: u8,
  pub artificial_ambient_occlusion: u8,
  pub tint_value: u32,
}
