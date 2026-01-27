use structdiff::{Difference, StructDiff};

use crate::core::common::position::Position;

/// Time cycle modifier definition in YMAP
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
pub struct YmapTimeCycleModifier {
  pub name: String,
  pub min_extents: Position,
  pub max_extents: Position,
  pub percentage: u32,
  pub range: f32,
  pub start_hour: u32,
  pub end_hour: u32,
}
