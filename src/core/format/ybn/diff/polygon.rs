use crate::core::format::ybn::model::Material;
use serde::{Deserialize, Serialize};

/// Collision shape expressed in its owning Bounds-local coordinates after GeometryCenter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResolvedShape {
  /// Triangle winding is preserved; cyclic corner shifts are equivalent.
  Triangle {
    /// Resolved corner positions.
    vertices: [[f32; 3]; 3],
    /// Flags parallel to corners.
    vertex_flags: [bool; 3],
  },
  /// Box reference vertices retain their defining order.
  Box {
    /// Resolved reference positions.
    vertices: [[f32; 3]; 4],
  },
  /// Sphere center and radius.
  Sphere {
    /// Center position.
    center: [f32; 3],
    /// Collision radius.
    radius: f32,
  },
  /// Capsule endpoints and radius.
  Capsule {
    /// Axis endpoints.
    endpoints: [[f32; 3]; 2],
    /// Collision radius.
    radius: f32,
  },
  /// Cylinder endpoints and radius.
  Cylinder {
    /// Axis endpoints.
    endpoints: [[f32; 3]; 2],
    /// Collision radius.
    radius: f32,
  },
}

/// A collision primitive with vertex and material references resolved by value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedPolygon {
  /// Geometry without BVH, polygon indices or derived triangle metadata.
  pub shape: ResolvedShape,
  /// Material properties; colour slot is normalized when its colour can be resolved.
  pub material: Material,
  /// Resolved material colour, when a palette is present.
  pub material_colour: Option<[u8; 4]>,
  /// Colours parallel to shape vertices, or empty when absent.
  pub vertex_colours: Vec<[u8; 4]>,
}

/// Provenance in one input model, not a cross-version polygon identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolygonLocation {
  /// Child slots from the root to the owning Bounds; empty denotes the root.
  pub bound_path: Vec<usize>,
  /// Input polygon position, useful for diagnostics and locating a change in that input.
  pub polygon_index: usize,
}

/// One added or removed occurrence, preserving duplicate-face multiplicity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum PolygonDiff {
  /// A primitive present only in the modified input.
  Added {
    /// Modified-input location.
    location: PolygonLocation,
    /// Resolved primitive.
    polygon: ResolvedPolygon,
  },
  /// A primitive present only in the baseline input.
  Removed {
    /// Baseline-input location.
    location: PolygonLocation,
    /// Resolved primitive.
    polygon: ResolvedPolygon,
  },
}

impl ResolvedShape {
  pub(super) fn positions(&self) -> &[[f32; 3]] {
    match self {
      Self::Triangle {
        vertices,
        ..
      } => vertices,
      Self::Box {
        vertices,
      } => vertices,
      Self::Sphere {
        center,
        ..
      } => std::slice::from_ref(center),
      Self::Capsule {
        endpoints,
        ..
      }
      | Self::Cylinder {
        endpoints,
        ..
      } => endpoints,
    }
  }

  pub(super) fn kind(&self) -> u8 {
    match self {
      Self::Triangle {
        ..
      } => 0,
      Self::Box {
        ..
      } => 1,
      Self::Sphere {
        ..
      } => 2,
      Self::Capsule {
        ..
      } => 3,
      Self::Cylinder {
        ..
      } => 4,
    }
  }
}
