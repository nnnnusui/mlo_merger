//! Loads and caches the YMAPs referenced while comparing source resources against vanilla.
//!
//! How each YMAP field is compared is declared on the model with `#[derive(Diff)]`, and how
//! array items pair up on their types, with `#[diff(key)]` or `#[diff(matching)]` next to the
//! function they name, and by equality otherwise. This module holds the comparisons the derive
//! cannot express: entities, grass and prop instances, distant LOD light positions and
//! physics dictionaries, entity extensions, LOD lights kept in step with their distant LOD
//! lights, and how the diffs of several sources combine.

mod context;
mod diff;
pub mod distant_lod_lights;
pub mod entities;
mod entity_parent;
pub mod extensions;
pub mod instanced_data;
pub mod lod_light_pairs;
mod merge;
mod merged_items;
#[cfg(test)]
mod test;

pub use context::{StreamFileRef, YmapComparisonContext, YmapParentRef};
pub use diff::{
  YmapComparisonDiff, YmapComparisonFile, YmapComparisonPartner, physics_dictionaries,
};
pub use entities::YmapEntityDiff;
pub use entity_parent::{EntityParent, EntityParents, YmapDiffContext, entity_parents};
pub use instanced_data::{
  YmapAddedGrassInstance, YmapGrassBatchDiff, YmapGrassBatchFieldDiff, YmapModifiedGrassInstance,
  YmapPropInstanceDiff,
};
pub use merge::merge_diffs;
pub use merged_items::NoFieldDiff;
pub use struct_diff::vec::ItemDiff;

pub use crate::{
  common::{
    position::PositionDiff,
    rotation::RotationDiff,
    triangle::{Triangle, TriangleDiff},
  },
  core::format::ymap::model::{
    YmapBoxOccluder, YmapCarGenerator, YmapDistantLodLight, YmapLodLight, YmapOccludeModel,
    YmapTimeCycleModifier,
    ymap::YmapDiffFields,
    ymap_block::YmapBlockDiff,
    ymap_box_occluder::YmapBoxOccluderFieldDiff,
    ymap_car_generator::YmapCarGeneratorFieldDiff,
    ymap_distant_lod_lights::{YmapDistantLodLightFieldDiff, YmapDistantLodLightsDiff},
    ymap_entity::YmapEntityFieldDiff,
    ymap_instanced_data::{YmapGrassInstanceFieldDiff, YmapInstancedDataDiff},
    ymap_lod_light::YmapLodLightFieldDiff,
    ymap_occlude_model::YmapOccludeModelFieldDiff,
    ymap_time_cycle_modifier::YmapTimeCycleModifierFieldDiff,
  },
};

/// How one box occluder of a modded YMAP differs from vanilla.
pub type YmapBoxOccluderDiff = ItemDiff<YmapBoxOccluder, YmapBoxOccluderFieldDiff>;
/// How one car generator of a modded YMAP differs from vanilla.
pub type YmapCarGeneratorDiff = ItemDiff<YmapCarGenerator, YmapCarGeneratorFieldDiff>;
/// How one distant LOD light of a modded YMAP differs from vanilla.
pub type YmapDistantLodLightDiff = ItemDiff<YmapDistantLodLight, YmapDistantLodLightFieldDiff>;
/// How one LOD light of a modded YMAP differs from vanilla.
pub type YmapLodLightDiff = ItemDiff<YmapLodLight, YmapLodLightFieldDiff>;
/// How one occlude model of a modded YMAP differs from vanilla.
pub type YmapOccludeModelDiff = ItemDiff<YmapOccludeModel, YmapOccludeModelFieldDiff>;
/// How one time cycle modifier of a modded YMAP differs from vanilla.
pub type YmapTimeCycleModifierDiff =
  ItemDiff<YmapTimeCycleModifier, YmapTimeCycleModifierFieldDiff>;
/// How one occlude model triangle of a modded YMAP differs from vanilla.
pub type YmapTriangleDiff = ItemDiff<Triangle, TriangleDiff>;
