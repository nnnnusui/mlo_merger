use structdiff::{Difference, StructDiff};

/// Box occluder definition in YMAP
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
pub struct YmapBoxOccluder {
  pub i_center_x: i32,
  pub i_center_y: i32,
  pub i_center_z: i32,
  pub i_cos_z: i32,
  pub i_length: u32,
  pub i_width: u32,
  pub i_height: u32,
  pub i_sin_z: i32,
}
