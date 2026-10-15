use structdiff::{Difference, StructDiff};

use crate::common::position::Position;

use super::{
  ymap_block::YmapBlock, ymap_box_occluder::YmapBoxOccluder, ymap_car_generator::YmapCarGenerator,
  ymap_distant_lod_lights::YmapDistantLodLights, ymap_entity::YmapEntity,
  ymap_instanced_data::YmapInstancedData, ymap_lod_light::YmapLodLight,
  ymap_occlude_model::YmapOccludeModel, ymap_time_cycle_modifier::YmapTimeCycleModifier,
};

#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Ymap {
  pub name: String,
  pub parent: String,
  pub flags: u32,
  pub content_flags: u32,
  #[difference(recurse)]
  pub streaming_extents_min: Position,
  pub streaming_extents_max: Position,
  pub entities_extents_min: Position,
  pub entities_extents_max: Position,
  pub entities: Vec<YmapEntity>,
  pub box_occluders: Vec<YmapBoxOccluder>,
  pub occlude_models: Vec<YmapOccludeModel>,
  pub lod_lights: Vec<YmapLodLight>,
  pub distant_lod_lights: YmapDistantLodLights,
  pub block: YmapBlock,
  pub container_lods: Vec<String>,
  pub physics_dictionaries: Vec<String>,
  pub instanced_data: YmapInstancedData,
  pub time_cycle_modifiers: Vec<YmapTimeCycleModifier>,
  pub car_generators: Vec<YmapCarGenerator>,
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

    let recursed = diffs
      .iter()
      .find_map(|diff| match diff {
        YmapStructDiffEnum::streaming_extents_min(inner) => Some(inner),
        _ => None,
      })
      .expect("streaming_extents_min diff");
    println!("streaming_extents_min inner diffs: {recursed:#?}");
    assert_eq!(recursed.len(), 2);

    assert_eq!(vanilla.apply(diffs), modded);
  }
}
