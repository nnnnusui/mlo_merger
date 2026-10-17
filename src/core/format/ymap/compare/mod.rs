//! Loads and caches the YMAPs referenced while comparing source resources against vanilla.
//!
//! How each YMAP field is compared is declared on the model with `#[derive(Diff)]`; this module
//! holds the `#[diff(with)]` strategies of the arrays, which pair their items before comparing
//! them.

pub mod box_occluders;
pub mod car_generators;
mod context;
mod data;
mod diff;
pub mod distant_lod_lights;
pub mod entities;
mod field;
pub mod instanced_data;
mod item_diff;
pub mod lod_lights;
mod merge;
pub mod occlude_models;
#[cfg(test)]
mod test;
pub mod time_cycle_modifiers;

pub use box_occluders::YmapBoxOccluderDiff;
pub use car_generators::YmapCarGeneratorDiff;
pub use context::{StreamFileRef, YmapComparisonContext};
pub use data::{YmapComparisonData, YmapParentRef};
pub use diff::{YmapComparisonDiff, YmapComparisonFile, physics_dictionaries};
pub use distant_lod_lights::{YmapDistantLodLightDiff, parse_distant_position};
pub use entities::YmapEntityDiff;
pub use instanced_data::{
  YmapAddedGrassInstance, YmapGrassBatchDiff, YmapGrassBatchFieldDiff, YmapModifiedGrassInstance,
  YmapPropInstanceDiff, grass_world_position,
};
pub use item_diff::{NoFieldDiff, YmapItemDiff};
pub use lod_lights::YmapLodLightDiff;
pub use merge::merge_diffs;
pub use occlude_models::{YmapOccludeModelDiff, triangles::YmapTriangleDiff};
pub use time_cycle_modifiers::YmapTimeCycleModifierDiff;

pub use crate::{
  common::{position::PositionDiff, rotation::RotationDiff},
  core::format::ymap::model::{
    ymap::YmapDiffFields,
    ymap_block::YmapBlockDiff,
    ymap_box_occluder::YmapBoxOccluderFieldDiff,
    ymap_car_generator::YmapCarGeneratorFieldDiff,
    ymap_distant_lod_lights::{YmapDistantLodLightFieldDiff, YmapDistantLodLightsDiff},
    ymap_entity::YmapEntityFieldDiff,
    ymap_instanced_data::{YmapGrassInstanceFieldDiff, YmapInstancedDataDiff},
    ymap_occlude_model::YmapOccludeModelFieldDiff,
    ymap_time_cycle_modifier::YmapTimeCycleModifierFieldDiff,
  },
};
