use structdiff::{Difference, StructDiff};

use crate::core::common::position::Position;

/// Car generator definition in YMAP
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
pub struct YmapCarGenerator {
  pub position: Position,
  pub orient_x: f32,
  pub orient_y: f32,
  pub perpendicular_length: f32,
  pub car_model: String,
  pub flags: u32,
  pub body_color_remap_1: i32,
  pub body_color_remap_2: i32,
  pub body_color_remap_3: i32,
  pub body_color_remap_4: i32,
  pub pop_group: String,
  pub livery: i32,
}
