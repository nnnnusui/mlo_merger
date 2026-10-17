//! Distant LOD light positions: `"x y z"` text compared as a `Position`.

/// The `#[diff(with)]` strategy of `YmapDistantLodLight::position`, comparing the `"x y z"`
/// text as a `Position` whose axes ignore changes below the tolerance.
///
/// Changes are only taken between parseable positions, so they are ignored when applied to a
/// position that cannot be parsed.
pub mod position {
  use struct_diff::{ApplyError, Diff};

  use crate::{common::position::PositionDiff, core::format::ymap::model::YmapDistantLodLight};

  pub fn diff(
    before: &str,
    after: &str,
    tolerance: &f32,
  ) -> Option<PositionDiff> {
    YmapDistantLodLight::parse_position(before)?
      .diff(&YmapDistantLodLight::parse_position(after)?, tolerance)
  }

  /// Rewrites `target` as `"x y z"` with the changed axes.
  pub fn apply(
    target: &mut String,
    diff: &PositionDiff,
  ) -> Result<(), ApplyError> {
    if let Some(mut position) = YmapDistantLodLight::parse_position(target) {
      position.apply(diff)?;
      *target = format!("{} {} {}", position.x, position.y, position.z);
    }
    Ok(())
  }
}
