use crate::{
  common::position::Position,
  core::format::ymap::compare::{
    self, YmapBoxOccluderDiff, YmapCarGeneratorDiff, YmapDiffContext, YmapEntityDiff,
    YmapLodLightDiff, YmapOccludeModelDiff, YmapTimeCycleModifierDiff,
  },
};

use super::{
  ymap_block::YmapBlock, ymap_box_occluder::YmapBoxOccluder, ymap_car_generator::YmapCarGenerator,
  ymap_container_lod::YmapContainerLod, ymap_distant_lod_lights::YmapDistantLodLights,
  ymap_entity::YmapEntity, ymap_instanced_data::YmapInstancedData, ymap_lod_light::YmapLodLight,
  ymap_occlude_model::YmapOccludeModel, ymap_time_cycle_modifier::YmapTimeCycleModifier,
};

#[derive(Debug, Clone, PartialEq, struct_diff::Diff, serde::Serialize, serde::Deserialize)]
#[diff(
  name = YmapDiffFields,
  context = YmapDiffContext,
  attr(cfg_attr(feature = "typescript", derive(specta::Type)))
)]
pub struct Ymap {
  pub name: String,
  pub parent: String,
  pub flags: u32,
  pub content_flags: u32,
  #[diff(nested, context = 0.0)]
  pub streaming_extents_min: Position,
  #[diff(nested, context = 0.0)]
  pub streaming_extents_max: Position,
  #[diff(nested, context = 0.0)]
  pub entities_extents_min: Position,
  #[diff(nested, context = 0.0)]
  pub entities_extents_max: Position,
  #[diff(nested, context = ())]
  pub block: YmapBlock,
  #[diff(with = compare::entities, output = Vec<YmapEntityDiff>)]
  pub entities: Vec<YmapEntity>,
  #[diff(with = compare::box_occluders, context = ctx.tolerances, output = Vec<YmapBoxOccluderDiff>)]
  pub box_occluders: Vec<YmapBoxOccluder>,
  #[diff(with = compare::occlude_models, context = ctx.tolerances, output = Vec<YmapOccludeModelDiff>)]
  pub occlude_models: Vec<YmapOccludeModel>,
  #[diff(with = compare::lod_lights, output = Vec<YmapLodLightDiff>)]
  pub lod_lights: Vec<YmapLodLight>,
  #[diff(nested, context = ctx.tolerances)]
  pub distant_lod_lights: YmapDistantLodLights,
  #[diff(with = compare::physics_dictionaries, output = Vec<String>)]
  pub physics_dictionaries: Vec<String>,
  #[diff(nested, context = ctx.tolerances)]
  pub instanced_data: YmapInstancedData,
  #[diff(with = compare::time_cycle_modifiers, context = ctx.tolerances, output = Vec<YmapTimeCycleModifierDiff>)]
  pub time_cycle_modifiers: Vec<YmapTimeCycleModifier>,
  #[diff(with = compare::car_generators, context = ctx.tolerances, output = Vec<YmapCarGeneratorDiff>)]
  pub car_generators: Vec<YmapCarGenerator>,
  /// Unused by every vanilla YMAP (0/12,584 across all vanilla versions) and every local
  /// source YMAP (0/700) as of 2026-10-16.
  #[diff(skip)]
  pub container_lods: Vec<YmapContainerLod>,
}

#[cfg(test)]
mod tests {
  use struct_diff::Diff;

  use super::*;
  use crate::core::format::ymap::model::{YmapBlock, YmapDistantLodLights, YmapInstancedData};

  fn create_test_ymap() -> Ymap {
    Ymap {
      name: String::new(),
      parent: String::new(),
      flags: 0,
      content_flags: 0,
      streaming_extents_min: Position::default(),
      streaming_extents_max: Position::default(),
      entities_extents_min: Position::default(),
      entities_extents_max: Position::default(),
      entities: Vec::new(),
      box_occluders: Vec::new(),
      occlude_models: Vec::new(),
      lod_lights: Vec::new(),
      distant_lod_lights: YmapDistantLodLights::default(),
      block: YmapBlock {
        version: 0,
        flags: 0,
        name: String::new(),
        exported_by: String::new(),
        owner: String::new(),
        time: String::new(),
      },
      container_lods: Vec::new(),
      physics_dictionaries: Vec::new(),
      instanced_data: YmapInstancedData::default(),
      time_cycle_modifiers: Vec::new(),
      car_generators: Vec::new(),
    }
  }

  #[test]
  fn extent_changes_round_trip_through_the_diff() {
    let vanilla = create_test_ymap();
    let mut modded = vanilla.clone();
    modded.streaming_extents_min.x = 1.0;
    modded.streaming_extents_min.z = 3.0;
    modded.streaming_extents_max.y = 2.0;

    let ctx = YmapDiffContext::new(Default::default(), (&vanilla, None), (&modded, None));
    let diff = vanilla.diff(&modded, &ctx).unwrap();
    let mut applied = vanilla.clone();
    applied.apply(&diff).unwrap();
    assert_eq!(applied, modded);
  }
}
