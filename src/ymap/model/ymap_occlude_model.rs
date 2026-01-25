use crate::common::Position;

/// Occlude model definition in YMAP
#[derive(Debug)]
pub struct YmapOccludeModel {
  pub bmin: Position,
  pub bmax: Position,
  pub data_size: u32,
  pub verts: Vec<u8>,
  pub num_verts_in_bytes: u32,
  pub num_tris: u32,
  pub flags: u32,
}
