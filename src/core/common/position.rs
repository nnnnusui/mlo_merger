use structdiff::{Difference, StructDiff};

#[derive(Debug, Clone, PartialEq, Difference)]
pub struct Position {
  pub x: f32,
  pub y: f32,
  pub z: f32,
}
