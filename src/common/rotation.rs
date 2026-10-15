/// Quaternion rotation (x, y, z, w)
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct Rotation {
  pub x: f32,
  pub y: f32,
  pub z: f32,
  pub w: f32,
}
