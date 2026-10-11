//! Versioned wire contracts for the separate FiveM light editor.

use serde::{Deserialize, Serialize};

#[cfg(feature = "typescript")]
type CoordinateTuple = (f64, f64, f64);
#[cfg(feature = "typescript")]
type AxesTuple = (CoordinateTuple, CoordinateTuple, CoordinateTuple);

/// World-space oriented enclosure, not an exact lighting or occlusion volume.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct LightObb {
  /// World-space center in GTA coordinate units.
  #[cfg_attr(feature = "typescript", specta(type = CoordinateTuple))]
  pub center: [f64; 3],
  /// Orthonormal local X, Y and Z axes in world coordinates.
  #[cfg_attr(feature = "typescript", specta(type = AxesTuple))]
  pub axes: [[f64; 3]; 3],
  /// Nonnegative distances from the center along each axis.
  #[cfg_attr(feature = "typescript", specta(type = CoordinateTuple))]
  pub half_extents: [f64; 3],
}

/// RBush-compatible world bounds retaining the vertical interval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct LightAabb {
  /// Minimum world X.
  pub min_x: f64,
  /// Minimum world Y.
  pub min_y: f64,
  /// Minimum world Z.
  pub min_z: f64,
  /// Maximum world X.
  pub max_x: f64,
  /// Maximum world Y.
  pub max_y: f64,
  /// Maximum world Z.
  pub max_z: f64,
}

/// The source snapshot required to interpret array indices safely.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct LightFile {
  /// Identifier local to this document.
  pub file_id: String,
  /// Original stream-relative path, independent of cache relocation.
  pub path: String,
  /// SHA-256 of the original native bytes.
  pub sha256: String,
  /// Original native byte length.
  pub size: u64,
}

/// Original zero-based array occurrence, valid only for its file snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct LightRef {
  /// File-table identifier.
  pub file_id: String,
  /// Original array index, before any removals.
  pub index: usize,
}

/// Numeric LOD properties and precomputed selection bounds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct LodLightView {
  /// Child LOD array occurrence.
  pub reference: LightRef,
  /// Emission direction, not world position.
  #[cfg_attr(feature = "typescript", specta(type = CoordinateTuple))]
  pub direction: [f64; 3],
  /// Native light hash; not a unique identity or a parent link.
  pub hash: u32,
  /// Native packed time, state and type flags.
  pub time_and_state_flags: u32,
  /// Point=1, spot=2, capsule=4.
  pub light_type: u32,
  /// Influence radius in world units.
  pub falloff: f64,
  /// Native attenuation exponent.
  pub falloff_exponent: f64,
  /// Native angle byte; radians are byte * PI / 255.
  pub cone_inner_angle: u8,
  /// Native outer-angle byte, or capsule half-length byte * 0.25.
  pub cone_outer_angle_or_cap_ext: u8,
  /// Native corona intensity byte.
  pub corona_intensity: u8,
  /// Conservative oriented influence enclosure.
  pub obb: LightObb,
  /// Bounds of the enclosure for spatial indexing.
  pub aabb: LightAabb,
}

/// One logical light joining a distant occurrence to its resource-local children.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct LightView {
  /// Snapshot-local removal identifier; never reuse after dependencies change.
  pub id: String,
  /// Parent distant array occurrence.
  pub distant_ref: LightRef,
  /// World-space source position.
  #[cfg_attr(feature = "typescript", specta(type = CoordinateTuple))]
  pub position: [f64; 3],
  /// Native packed colour/intensity.
  pub rgbi: u32,
  /// All linked child occurrences and their individual influence volumes.
  pub lod: Vec<LodLightView>,
  /// False when correspondence or geometry cannot be validated.
  pub editable: bool,
}

/// A noneditable source or relationship with an actionable explanation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct LightDiagnostic {
  /// Original stream-relative path.
  pub path: String,
  /// Explanation of the unsupported source data or relationship.
  pub message: String,
}

/// Lightweight YMAP light export consumed by an external editor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapLightDocument {
  /// Fixed discriminator: `mlo_ymap_lights`.
  pub format: String,
  /// Contract version, currently 1.
  pub schema_version: u32,
  /// Source-inventory resource identifier.
  pub resource: String,
  /// Requested original stream-relative YMAP path.
  pub path: String,
  /// Files required to interpret the exported relationships.
  pub files: Vec<LightFile>,
  /// Logical lights, preserving duplicate occurrences.
  pub lights: Vec<LightView>,
  /// Missing or unsupported relationships; no fallback is inferred.
  pub diagnostics: Vec<LightDiagnostic>,
}

/// Supported initial editor operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum LightOperation {
  /// Remove the logical light and every linked array occurrence.
  Remove {
    /// Identifier copied from the matching export snapshot.
    #[serde(rename = "lightId")]
    light_id: String,
  },
}

/// Resource-scoped edits, separate from semantic merge reports and vanilla deltas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapLightEditDocument {
  /// Fixed discriminator: `mlo_lod_light_edits`.
  pub format: String,
  /// Contract version, currently 1.
  pub schema_version: u32,
  /// Resource whose contribution is edited.
  pub resource: String,
  /// Original stream-relative target path.
  pub path: String,
  /// Dependency identities copied from the light export.
  pub files: Vec<LightFile>,
  /// Removal commands; repeated IDs are idempotent.
  pub operations: Vec<LightOperation>,
}

/// Original source paths superseded by resource-local light output files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct LightEditProvenance {
  /// Persisted contract version, currently 1.
  pub format_version: u32,
  /// Source-root-relative original paths to omit, including unmoved custom maps.
  pub omitted: std::collections::BTreeSet<String>,
}
