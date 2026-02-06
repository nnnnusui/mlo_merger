use structdiff::{Difference, StructDiff};

use crate::core::common::{position::Position, triangle::Triangle};

/// Occlude model definition in YMAP
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
pub struct YmapOccludeModel {
  pub bmin: Position,
  pub bmax: Position,
  pub triangles: Vec<Triangle>,
  pub flags: u32,
}

impl YmapOccludeModel {
  pub fn is_same(
    &self,
    other: &YmapOccludeModel,
  ) -> bool {
    const EPSILON: f32 = 0.01;
    let is_position_similar =
      |p1: &crate::core::common::position::Position,
       p2: &crate::core::common::position::Position| {
        (p1.x - p2.x).abs() < EPSILON
          && (p1.y - p2.y).abs() < EPSILON
          && (p1.z - p2.z).abs() < EPSILON
      };

    is_position_similar(&self.bmin, &other.bmin) || is_position_similar(&self.bmax, &other.bmax)
  }
}
