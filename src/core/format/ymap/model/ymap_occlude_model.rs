use structdiff::{Difference, StructDiff};

use crate::core::common::Position;

/// Occlude model definition in YMAP
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
pub struct YmapOccludeModel {
  pub bmin: Position,
  pub bmax: Position,
  pub data_size: u32,
  pub verts: Vec<u8>,
  pub num_verts_in_bytes: u32,
  pub num_tris: u32,
  pub flags: u32,
}
