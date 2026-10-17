use structdiff::{Difference, StructDiff};

/// Quaternion rotation (x, y, z, w)
#[derive(
  Clone,
  Debug,
  Default,
  Difference,
  PartialEq,
  serde::Serialize,
  serde::Deserialize,
  struct_diff::Diff,
)]
#[difference(expose)]
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
