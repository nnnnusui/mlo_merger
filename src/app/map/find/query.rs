//! The conditions a search selects entities, collision shapes, file bounds or occluders by.

use super::Result;
use crate::core::format::ymap::model::YmapEntity;

/// Condition selecting YMAP entities, collision shapes, file bounds or occluders.
#[derive(Debug, Clone, PartialEq)]
pub enum EntityQuery {
  /// Select every occurrence of this unsigned entity GUID.
  Guid(u32),
  /// Select entity positions within an inclusive three-dimensional radius.
  Position {
    /// Center coordinates, rounded to YMAP's f32 coordinate precision.
    position: [f64; 3],
    /// Finite nonnegative distance from the center.
    radius: f64,
  },
  /// Select YBN collision centers within an inclusive three-dimensional radius.
  YbnPosition {
    /// World-space search center at native coordinate precision.
    position: [f64; 3],
    /// Finite nonnegative distance from the center.
    radius: f64,
    /// Optional lowercase shape type: box, triangle, sphere, capsule or cylinder.
    kind: Option<String>,
  },
  /// Select files whose declared spatial bounds contain the position.
  FileFromPosition {
    /// World-space position at native coordinate precision.
    position: [f64; 3],
  },
  /// Select YMAP box occluders and occlude models within an inclusive radius.
  OccludePosition {
    /// World-space search center at native coordinate precision.
    position: [f64; 3],
    /// Finite nonnegative distance from the center or model bounds.
    radius: f64,
  },
}

impl EntityQuery {
  pub(super) fn validate(&self) -> Result<()> {
    if let Self::Position {
      position,
      radius,
    }
    | Self::YbnPosition {
      position,
      radius,
      ..
    } = self
      && (!position.iter().all(|value| value.is_finite() && (*value as f32).is_finite())
        || !radius.is_finite()
        || *radius < 0.0)
    {
      return Err("Position and radius must be finite; radius must be nonnegative".into());
    }
    if let Self::FileFromPosition {
      position,
    } = self
      && !position.iter().all(|value| value.is_finite() && (*value as f32).is_finite())
    {
      return Err("Position coordinates must be finite and in range".into());
    }
    if let Self::OccludePosition {
      position,
      radius,
    } = self
      && (!position.iter().all(|value| value.is_finite() && (*value as f32).is_finite())
        || !radius.is_finite()
        || *radius < 0.0)
    {
      return Err("Position and radius must be finite; radius must be nonnegative".into());
    }
    if let Self::YbnPosition {
      kind: Some(kind),
      ..
    } = self
      && !matches!(kind.as_str(), "box" | "triangle" | "sphere" | "capsule" | "cylinder")
    {
      return Err("YBN type must be box, triangle, sphere, capsule or cylinder".into());
    }
    Ok(())
  }

  pub(super) fn contains(
    &self,
    entity: &YmapEntity,
  ) -> bool {
    match self {
      Self::Guid(guid) => entity.guid == *guid,
      Self::YbnPosition {
        ..
      }
      | Self::FileFromPosition {
        ..
      } => false,
      Self::OccludePosition {
        ..
      } => false,
      Self::Position {
        position,
        radius,
      } => {
        let center = position.map(|value| value as f32 as f64);
        let delta = [
          entity.position.x as f64 - center[0],
          entity.position.y as f64 - center[1],
          entity.position.z as f64 - center[2],
        ];
        delta[0].hypot(delta[1]).hypot(delta[2]) <= *radius
      }
    }
  }
}

/// Whether the sphere of `radius` around `position` meets the box from `minimum` to `maximum`,
/// boundary included; `None` when any value is not finite or the box is inverted.
pub(super) fn aabb_intersects_radius(
  position: [f64; 3],
  radius: f64,
  minimum: [f64; 3],
  maximum: [f64; 3],
) -> Option<bool> {
  if (0..3).any(|axis| {
    !position[axis].is_finite()
      || !minimum[axis].is_finite()
      || !maximum[axis].is_finite()
      || minimum[axis] > maximum[axis]
  }) {
    return None;
  }
  let nearest: [f64; 3] =
    std::array::from_fn(|axis| position[axis].clamp(minimum[axis], maximum[axis]));
  let delta: [f64; 3] = std::array::from_fn(|axis| position[axis] - nearest[axis]);
  Some(delta[0].hypot(delta[1]).hypot(delta[2]) <= radius)
}
