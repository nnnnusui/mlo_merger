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

/// The `#[diff(with)]` strategy of entity rotations, comparing them by the turn they make: a
/// quaternion and its negation turn alike, so a rotation within the tolerance of the other's
/// negation is unchanged. A tolerance of zero compares exactly.
pub mod turn {
  use struct_diff::Diff;
  pub use struct_diff::nested::apply;

  use super::{Rotation, RotationDiff};

  pub fn diff(
    before: &Rotation,
    after: &Rotation,
    tolerance: &f32,
  ) -> Option<RotationDiff> {
    let negated = Rotation {
      x: -after.x,
      y: -after.y,
      z: -after.z,
      w: -after.w,
    };
    if *tolerance > 0.0 && before.diff(&negated, tolerance).is_none() {
      return None;
    }
    before.diff(after, tolerance)
  }
}
