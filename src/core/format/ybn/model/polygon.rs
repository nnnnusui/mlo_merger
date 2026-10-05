use serde::{Deserialize, Serialize};

/// Indexed triangle with corner flags and derived binary metadata.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Triangle {
  /// Material-table slot.
  pub material: u8,
  /// Derived triangle area.
  pub area: f32,
  /// Shared vertex references in winding order.
  pub vertices: [u16; 3],
  /// Flags associated with each corner.
  pub vertex_flags: [bool; 3],
  /// Derived neighboring polygon references.
  pub edge_indices: [u16; 3],
}

/// Native indexed collision primitives.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Polygon {
  /// Triangle using shared vertices and a material slot.
  Triangle(Triangle),
  /// Box defined by four reference vertices.
  Box {
    /// Material-table slot.
    material: u8,
    /// Shared vertex references.
    vertices: [u16; 4],
  },
  /// Sphere centered at a reference vertex.
  Sphere {
    /// Material-table slot.
    material: u8,
    /// Center vertex reference.
    vertex: u16,
    /// Collision radius.
    radius: f32,
  },
  /// Capsule between two reference vertices.
  Capsule {
    /// Material-table slot.
    material: u8,
    /// First endpoint reference.
    vertex1: u16,
    /// Second endpoint reference.
    vertex2: u16,
    /// Collision radius.
    radius: f32,
  },
  /// Cylinder between two reference vertices.
  Cylinder {
    /// Material-table slot.
    material: u8,
    /// First endpoint reference.
    vertex1: u16,
    /// Second endpoint reference.
    vertex2: u16,
    /// Collision radius.
    radius: f32,
  },
  /// Opaque record that semantic diff extraction must reject explicitly.
  Unsupported {
    /// Original polygon payload.
    raw: [u8; 16],
  },
}

impl Polygon {
  /// Returns the decoded material slot; opaque records have no decoded slot.
  pub fn material(&self) -> u8 {
    match self {
      Self::Triangle(polygon) => polygon.material,
      Self::Box {
        material,
        ..
      }
      | Self::Sphere {
        material,
        ..
      }
      | Self::Capsule {
        material,
        ..
      }
      | Self::Cylinder {
        material,
        ..
      } => *material,
      Self::Unsupported {
        ..
      } => 0,
    }
  }

  pub(crate) fn set_material_index(
    &mut self,
    index: u8,
  ) {
    match self {
      Self::Triangle(polygon) => polygon.material = index,
      Self::Box {
        material,
        ..
      }
      | Self::Sphere {
        material,
        ..
      }
      | Self::Capsule {
        material,
        ..
      }
      | Self::Cylinder {
        material,
        ..
      } => *material = index,
      Self::Unsupported {
        ..
      } => {}
    }
  }
}
