use indexmap::IndexMap;
use structdiff::{Difference, StructDiff};

use crate::core::common::position::Position;

use super::{
  ymap_block::YmapBlock, ymap_box_occluder::YmapBoxOccluder, ymap_car_generator::YmapCarGenerator,
  ymap_distant_lod_lights::YmapDistantLodLightsSoa, ymap_entity::YmapEntity,
  ymap_instanced_data::YmapInstancedData, ymap_lod_lights::YmapLodLightsSoa,
  ymap_occlude_model::YmapOccludeModel, ymap_time_cycle_modifier::YmapTimeCycleModifier,
};

#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
pub struct Ymap {
  pub name: String,
  pub parent: String,
  pub flags: u32,
  pub content_flags: u32,
  pub streaming_extents_min: Position,
  pub streaming_extents_max: Position,
  pub entities_extents_min: Position,
  pub entities_extents_max: Position,
  pub entity_map: IndexMap<u32, YmapEntity>,
  pub box_occluders: Vec<YmapBoxOccluder>,
  pub occlude_models: Vec<YmapOccludeModel>,
  pub lod_lights_soa: YmapLodLightsSoa,
  pub distant_lod_lights_soa: YmapDistantLodLightsSoa,
  pub block: YmapBlock,
  pub container_lods: Vec<String>,
  pub physics_dictionaries: Vec<String>,
  pub instanced_data: YmapInstancedData,
  pub time_cycle_modifiers: Vec<YmapTimeCycleModifier>,
  pub car_generators: Vec<YmapCarGenerator>,
}
