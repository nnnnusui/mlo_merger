use serde::{Deserialize, Serialize};

/// Collision material properties, independent of material table ordering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Material {
  /// Surface material type.
  pub kind: u8,
  /// Procedural material identifier.
  pub procedural_id: u8,
  /// Packed room identifier.
  pub room_id: u8,
  /// Packed pedestrian density.
  pub ped_density: u8,
  /// Collision behaviour flags.
  pub flags: u16,
  /// Material colour-table index.
  pub colour_index: u8,
  /// Preserved unknown material bits.
  pub unknown: u16,
}
