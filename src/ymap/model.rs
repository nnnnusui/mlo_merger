use crate::common::Position;

#[derive(Debug)]
pub struct Ymap {
  pub name: String,
  pub parent: String,
  pub flags: u32,
  pub content_flags: u32,
  pub streaming_extents_min: Position,
  pub streaming_extents_max: Position,
  pub entities_extents_min: Position,
  pub entities_extents_max: Position,
}
