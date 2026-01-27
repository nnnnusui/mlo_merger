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
