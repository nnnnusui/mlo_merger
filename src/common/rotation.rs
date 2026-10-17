/// Quaternion rotation (x, y, z, w)
#[derive(
  Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize, struct_diff::Diff,
)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[diff(
  context = f32,
  attr(cfg_attr(feature = "typescript", derive(specta::Type)))
)]
pub struct Rotation {
  #[diff(threshold = *ctx)]
  pub x: f32,
  #[diff(threshold = *ctx)]
  pub y: f32,
  #[diff(threshold = *ctx)]
  pub z: f32,
  #[diff(threshold = *ctx)]
  pub w: f32,
}
