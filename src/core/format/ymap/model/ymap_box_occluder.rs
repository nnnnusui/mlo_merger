use structdiff::{Difference, StructDiff};

/// Box occluder definition in YMAP
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
#[derive(serde::Serialize, serde::Deserialize)]
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

impl YmapBoxOccluder {
  pub fn is_same(
    &self,
    other: &YmapBoxOccluder,
  ) -> bool {
    const EPSILON: i32 = 1;
    (self.i_center_x - other.i_center_x).abs() < EPSILON
      && (self.i_center_y - other.i_center_y).abs() < EPSILON
      && (self.i_center_z - other.i_center_z).abs() < EPSILON
  }
}
