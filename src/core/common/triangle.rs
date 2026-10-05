use structdiff::{Difference, StructDiff};

use crate::core::common::position::Position;

#[derive(Debug, Clone, PartialEq, Difference, serde::Serialize, serde::Deserialize)]
pub struct Triangle {
  pub corner_1: Position,
  pub corner_2: Position,
  pub corner_3: Position,
}
