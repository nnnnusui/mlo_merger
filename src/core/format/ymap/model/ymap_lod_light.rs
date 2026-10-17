use crate::common::position::Position;

/// LOD lights structure of array in YMAP
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
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
