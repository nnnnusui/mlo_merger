//! Entity extensions: data attached to one entity, such as the lights of a lamp.
//!
//! Only these four types occur in vanilla and local source YMAPs (as of 2026-10-21); YMAPs
//! with other extension types fail to decode rather than lose them.

use crate::common::position::Position;

/// One extension of an entity, by its META structure.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum YmapExtension {
  /// `CExtensionDefLightEffect`.
  LightEffect(YmapLightEffect),
  /// `CExtensionDefSpawnPointOverride`.
  SpawnPointOverride(YmapSpawnPointOverride),
  /// `CExtensionDefDoor`.
  Door(YmapDoor),
  /// `rage__phVerletClothCustomBounds`.
  VerletClothCustomBounds(YmapVerletClothCustomBounds),
}

/// Lights an entity emits, positioned relative to it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapLightEffect {
  pub name: String,
  pub offset_position: Position,
  pub instances: Vec<YmapLightAttr>,
}

/// One light of a light effect (`CLightAttrDef`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapLightAttr {
  pub posn: [f32; 3],
  pub colour: [u8; 3],
  pub flashiness: u8,
  pub intensity: f32,
  pub flags: u32,
  pub bone_tag: i16,
  pub light_type: u8,
  pub group_id: u8,
  pub time_flags: u32,
  pub falloff: f32,
  pub falloff_exponent: f32,
  pub culling_plane: [f32; 4],
  pub shadow_blur: u8,
  pub padding_1: u8,
  pub padding_2: i16,
  pub padding_3: u32,
  pub vol_intensity: f32,
  pub vol_size_scale: f32,
  pub vol_outer_colour: [u8; 3],
  pub light_hash: u8,
  pub vol_outer_intensity: f32,
  pub corona_size: f32,
  pub vol_outer_exponent: f32,
  pub light_fade_distance: u8,
  pub shadow_fade_distance: u8,
  pub specular_fade_distance: u8,
  pub volumetric_fade_distance: u8,
  pub shadow_near_clip: f32,
  pub corona_intensity: f32,
  pub corona_z_bias: f32,
  pub direction: [f32; 3],
  pub tangent: [f32; 3],
  pub cone_inner_angle: f32,
  pub cone_outer_angle: f32,
  pub extents: [f32; 3],
  pub projected_texture_key: u32,
}

/// A scenario spawn point of an entity, overriding its archetype's.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapSpawnPointOverride {
  pub name: String,
  pub offset_position: Position,
  pub scenario_type: String,
  pub i_time_start_override: u8,
  pub i_time_end_override: u8,
  pub group: String,
  pub model_set: String,
  /// A `CSpawnPoint__AvailabilityMpSp` name.
  pub availability_in_mp_sp: String,
  /// `CScenarioPointFlags__Flags` names, comma separated.
  pub flags: String,
  pub radius: f32,
  pub time_till_ped_leaves: f32,
}

/// How an entity opens as a door.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapDoor {
  pub name: String,
  pub offset_position: Position,
  pub enable_limit_angle: bool,
  pub starts_locked: bool,
  pub can_break: bool,
  pub limit_angle: f32,
  pub door_target_ratio: f32,
  pub audio_hash: String,
}

/// Capsules bounding the cloth of an entity.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapVerletClothCustomBounds {
  pub name: String,
  pub collision_data: Vec<YmapCapsuleBound>,
}

/// One capsule of cloth bounds (`rage__phCapsuleBoundDef`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapCapsuleBound {
  pub owner_name: String,
  pub rotation: [f32; 4],
  pub position: Position,
  pub normal: Position,
  pub capsule_radius: f32,
  pub capsule_len: f32,
  pub capsule_half_height: f32,
  pub capsule_half_width: f32,
  /// `rage__phCapsuleBoundDef__enCollisionBoundDef` names, comma separated.
  pub flags: String,
}
