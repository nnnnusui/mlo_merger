use structdiff::{Difference, StructDiff};

/// Direction vector for LOD lights
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
pub struct Direction {
  pub x: f32,
  pub y: f32,
  pub z: f32,
}

/// LOD lights structure of array in YMAP
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
#[derive(Default)]
pub struct YmapLodLightsSoa {
  pub direction: Vec<Direction>,
  pub falloff: Vec<String>,
  pub falloff_exponent: Vec<String>,
  pub time_and_state_flags: Vec<String>,
  pub hash: Vec<String>,
  pub cone_inner_angle: Vec<String>,
  pub cone_outer_angle_or_cap_ext: Vec<String>,
  pub corona_intensity: Vec<String>,
}
