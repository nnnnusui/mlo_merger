//! LOD light differences.
//!
//! LOD lights are compared on their own, without their parent's distant LOD light positions.
//! They carry no identity of their own, so they are compared exactly as a multiset: lights the
//! source lacks are removed by their vanilla index, lights only the source has are added in full.

use super::{NoFieldDiff, YmapItemDiff, field::pair_by_keys};
use crate::core::format::ymap::model::{Ymap, YmapLodLight};

/// How one LOD light of a modded YMAP differs from vanilla.
pub type YmapLodLightDiff = YmapItemDiff<YmapLodLight, NoFieldDiff>;

/// Every field of a LOD light, with floats as bits, so equal lights share a key.
fn light_key(light: &YmapLodLight) -> impl std::hash::Hash + Eq + use<> {
  (
    [light.direction.x, light.direction.y, light.direction.z].map(f32::to_bits),
    [light.falloff, light.falloff_exponent, light.corona_intensity].map(f32::to_bits),
    light.time_and_state_flags.clone(),
    light.hash.clone(),
    light.cone_inner_angle,
    light.cone_outer_angle_or_cap_ext,
  )
}

/// Collects `lod_lights` differences.
pub fn lod_light_diffs(
  vanilla: &Ymap,
  modded: &Ymap,
) -> Vec<YmapLodLightDiff> {
  let keys = |lights: &[YmapLodLight]| lights.iter().map(|light| Some(light_key(light))).collect();
  let pairing = pair_by_keys(keys(&vanilla.lod_lights), keys(&modded.lod_lights));
  YmapItemDiff::unpaired(pairing, &modded.lod_lights)
}
