//! Loads and caches the YMAPs referenced while comparing source resources against vanilla.

mod context;
mod data;
mod diff;
mod field;
mod item_diff;
#[cfg(test)]
mod test;
mod ymap_box_occluder_diff;
mod ymap_car_generator_diff;
mod ymap_distant_lod_light_diff;
mod ymap_entity_diff;
mod ymap_instanced_data_diff;
mod ymap_lod_light_diff;
mod ymap_occlude_model_diff;
mod ymap_time_cycle_modifier_diff;

pub use context::{StreamFileRef, YmapComparisonContext};
pub use data::{YmapComparisonData, YmapParentRef};
pub use diff::{
  ExtentsDiff, PositionDiff, RotationDiff, YmapBlockDiff, YmapComparisonDiff, YmapComparisonFile,
  YmapDiffFields, added_physics_dictionaries,
};
pub use item_diff::{NoFieldDiff, YmapItemDiff};
pub use ymap_box_occluder_diff::{
  YmapBoxOccluderDiff, YmapBoxOccluderFieldDiff, box_occluder_diffs,
};
pub use ymap_car_generator_diff::{
  YmapCarGeneratorDiff, YmapCarGeneratorFieldDiff, car_generator_diffs,
};
pub use ymap_distant_lod_light_diff::{
  YmapDistantLodLightDiff, YmapDistantLodLightFieldDiff, YmapDistantLodLightsDiff,
  distant_lod_lights_diff, parse_distant_position,
};
use ymap_entity_diff::apply_entity_diffs;
pub use ymap_entity_diff::{YmapEntityDiff, YmapEntityFieldDiff, entity_diffs};
pub use ymap_instanced_data_diff::{
  YmapAddedGrassInstance, YmapGrassBatchDiff, YmapGrassBatchFieldDiff, YmapGrassInstanceFieldDiff,
  YmapInstancedDataDiff, YmapModifiedGrassInstance, YmapPropInstanceDiff, grass_world_position,
  instanced_data_diff,
};
pub use ymap_lod_light_diff::{YmapLodLightDiff, lod_light_diffs};
pub use ymap_occlude_model_diff::{
  YmapOccludeModelDiff, YmapOccludeModelFieldDiff, occlude_model_diffs,
};
pub use ymap_time_cycle_modifier_diff::{
  YmapTimeCycleModifierDiff, YmapTimeCycleModifierFieldDiff, time_cycle_modifier_diffs,
};
