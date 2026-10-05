use super::{Material, Polygon};
use serde::{Deserialize, Serialize};

/// Shared vertex/material tables and polygons; BVH acceleration data is regenerated.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Geometry {
  /// Offset added to decoded local vertices.
  pub center: [f32; 3],
  /// Preserved scalar at native offset 0x9c.
  pub unknown_9c: f32,
  /// Preserved scalar at native offset 0xac.
  pub unknown_ac: f32,
  /// Quantization grid for packed vertex coordinates.
  pub vertex_quantum: Option<[f32; 3]>,
  /// Positions before GeometryCenter and parent transforms.
  pub vertices: Vec<[f32; 3]>,
  /// Materials referenced by polygon slots.
  pub materials: Vec<Material>,
  /// Optional material colour table.
  pub material_colours: Vec<[u8; 4]>,
  /// Optional colours parallel to vertices.
  pub vertex_colours: Vec<[u8; 4]>,
  /// Indexed collision primitives.
  pub polygons: Vec<Polygon>,
}
