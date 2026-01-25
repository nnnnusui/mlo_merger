/// LOD lights structure of array in YMAP
#[derive(Debug, Clone, PartialEq)]
pub struct YmapLodLightsSoa {
  pub direction: Vec<String>,
  pub falloff: Vec<String>,
  pub falloff_exponent: Vec<String>,
  pub time_and_state_flags: Vec<String>,
  pub hash: Vec<String>,
  pub cone_inner_angle: Vec<String>,
  pub cone_outer_angle_or_cap_ext: Vec<String>,
  pub corona_intensity: Vec<String>,
}
