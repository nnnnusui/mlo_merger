use structdiff::{Difference, StructDiff};

use crate::{
  common::position::Position,
  core::{
    config::matching::MatchTolerances,
    format::ymap::compare::{
      self, YmapBoxOccluderDiff, YmapCarGeneratorDiff, YmapEntityDiff, YmapLodLightDiff,
      YmapOccludeModelDiff, YmapTimeCycleModifierDiff,
    },
  },
};

use super::{
  ymap_block::YmapBlock, ymap_box_occluder::YmapBoxOccluder, ymap_car_generator::YmapCarGenerator,
  ymap_container_lod::YmapContainerLod, ymap_distant_lod_lights::YmapDistantLodLights,
  ymap_entity::YmapEntity, ymap_instanced_data::YmapInstancedData, ymap_lod_light::YmapLodLight,
  ymap_occlude_model::YmapOccludeModel, ymap_time_cycle_modifier::YmapTimeCycleModifier,
};

#[derive(Debug, Clone, PartialEq, Difference, struct_diff::Diff)]
#[difference(expose)]
#[derive(serde::Serialize, serde::Deserialize)]
#[diff(
  name = YmapDiffFields,
  context = MatchTolerances,
  attr(cfg_attr(feature = "typescript", derive(specta::Type)))
)]
pub struct Ymap {
  pub name: String,
  pub parent: String,
  pub flags: u32,
  pub content_flags: u32,
  #[difference(recurse)]
  #[diff(nested, context = 0.0)]
  pub streaming_extents_min: Position,
  #[difference(recurse)]
  #[diff(nested, context = 0.0)]
  pub streaming_extents_max: Position,
  #[difference(recurse)]
  #[diff(nested, context = 0.0)]
  pub entities_extents_min: Position,
  #[difference(recurse)]
  #[diff(nested, context = 0.0)]
  pub entities_extents_max: Position,
  #[difference(recurse)]
  #[diff(nested, context = ())]
  pub block: YmapBlock,
  #[difference(skip)]
  #[diff(with = compare::entities, output = Vec<YmapEntityDiff>)]
  pub entities: Vec<YmapEntity>,
  #[difference(skip)]
  #[diff(with = compare::box_occluders, output = Vec<YmapBoxOccluderDiff>)]
  pub box_occluders: Vec<YmapBoxOccluder>,
  #[difference(skip)]
  #[diff(with = compare::occlude_models, output = Vec<YmapOccludeModelDiff>)]
  pub occlude_models: Vec<YmapOccludeModel>,
  #[difference(skip)]
  #[diff(with = compare::lod_lights, output = Vec<YmapLodLightDiff>)]
  pub lod_lights: Vec<YmapLodLight>,
  #[difference(skip)]
  #[diff(nested)]
  pub distant_lod_lights: YmapDistantLodLights,
  #[difference(skip)]
  #[diff(with = compare::physics_dictionaries, output = Vec<String>)]
  pub physics_dictionaries: Vec<String>,
  #[difference(skip)]
  #[diff(nested)]
  pub instanced_data: YmapInstancedData,
  #[difference(skip)]
  #[diff(with = compare::time_cycle_modifiers, output = Vec<YmapTimeCycleModifierDiff>)]
  pub time_cycle_modifiers: Vec<YmapTimeCycleModifier>,
  #[difference(skip)]
  #[diff(with = compare::car_generators, output = Vec<YmapCarGeneratorDiff>)]
  pub car_generators: Vec<YmapCarGenerator>,
  /// Unused by every vanilla YMAP (0/12,584 across all vanilla versions) and every local
  /// source YMAP (0/700) as of 2026-10-16.
  #[difference(skip)]
  #[diff(skip)]
  pub container_lods: Vec<YmapContainerLod>,
}

#[cfg(test)]
mod tests {
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
  fn print_recursive_streaming_extents_min_diff() {
    let vanilla = create_test_ymap();
    let mut modded = vanilla.clone();
    modded.streaming_extents_min.x = 1.0;
    modded.streaming_extents_min.z = 3.0;
    // non-recursive field for comparison
    modded.streaming_extents_max.y = 2.0;

    let diffs = vanilla.diff(&modded);
    println!("{diffs:#?}");

    assert_eq!(vanilla.apply(diffs), modded);
  }
}
