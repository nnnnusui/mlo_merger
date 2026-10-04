use structdiff::{Difference, StructDiff};

use crate::core::common::position::Position;

/// LOD lights structure of array in YMAP
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
#[derive(Default)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct YmapLodLight {
  pub direction: Position,
  pub falloff: f32,
  pub falloff_exponent: f32,
  pub time_and_state_flags: String,
  pub hash: String,
  pub cone_inner_angle: u32,
  pub cone_outer_angle_or_cap_ext: u32,
  pub corona_intensity: f32,
}
