use super::Geometry;
use serde::{Deserialize, Serialize};

/// A Bounds node with its hierarchy and codec-preserved record fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bound {
  /// Native Bounds type name.
  pub kind: String,
  /// Common record bytes, including metadata not yet exposed as named fields.
  pub common: Vec<u8>,
  /// Preserved type-specific extension bytes.
  pub extension: Vec<u8>,
  /// Indexed Geometry or GeometryBVH payload.
  pub geometry: Option<Geometry>,
  /// Ordered child Bounds, including empty slots.
  pub children: Vec<Bound>,
  /// Local transform relative to the owning Composite.
  pub transform: Option<[f32; 16]>,
  /// Child collision filter flags.
  pub composite_flags: [u32; 2],
  /// Composite child transforms parallel to children.
  pub transforms: Vec<[f32; 16]>,
  /// Composite child collision filters parallel to children.
  pub flags: Vec<[u32; 2]>,
}
