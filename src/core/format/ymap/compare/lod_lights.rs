//! LOD light differences, the `#[diff(with)]` strategy of `Ymap::lod_lights`.
//!
//! LOD lights are compared on their own, without their parent's distant LOD light positions.
//! They carry no identity of their own, so they are compared exactly as a multiset: lights the
//! source lacks are removed by their vanilla index, lights only the source has are added in full.

use struct_diff::ApplyError;

use super::{NoFieldDiff, YmapItemDiff, field::pair_by_keys};
use crate::core::format::ymap::model::YmapLodLight;

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

/// Lights only one of `vanilla` and `modded` has.
pub fn diff<C: ?Sized>(
  vanilla: &[YmapLodLight],
  modded: &[YmapLodLight],
  _ctx: &C,
) -> Option<Vec<YmapLodLightDiff>> {
  let keys = |lights: &[YmapLodLight]| lights.iter().map(|light| Some(light_key(light))).collect();
  let diffs = YmapItemDiff::unpaired(pair_by_keys(keys(vanilla), keys(modded)), modded);
  (!diffs.is_empty()).then_some(diffs)
}

pub fn apply(
  items: &mut Vec<YmapLodLight>,
  diffs: &[YmapLodLightDiff],
) -> Result<(), ApplyError> {
  YmapItemDiff::apply_unpaired(items, diffs)
}
