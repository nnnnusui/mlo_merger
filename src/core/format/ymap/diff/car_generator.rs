use crate::core::{
  common::position::Position,
  format::ymap::model::{Ymap, YmapCarGenerator},
};
use std::collections::{HashMap, HashSet};

#[derive(serde::Serialize, serde::Deserialize)]
pub enum YmapCarGeneratorDiff {
  Added(YmapCarGenerator),
  Removed(YmapCarGenerator),
  Modified {
    vanilla: YmapCarGenerator,
    modded: YmapCarGenerator,
  },
}

impl YmapCarGeneratorDiff {
  pub fn extract_from(
    vanilla: &Ymap,
    modded: &Ymap,
  ) -> Vec<YmapCarGeneratorDiff> {
    check_car_generator_diff(vanilla, modded)
  }

  pub fn print_diffs(diffs: &Vec<YmapCarGeneratorDiff>) {
    log::info!("    Found {} car generator differences", diffs.len());
    for diff in diffs {
      match diff {
        Self::Added(generator) => {
          log::info!("      [Added] Car Generator at: {:?}", generator.position);
        }
        Self::Removed(generator) => {
          log::info!("      [Removed] Car Generator at: {:?}", generator.position);
        }
        Self::Modified {
          vanilla,
          modded: _,
        } => {
          log::info!("      [Modified] Car Generator at: {:?}", vanilla.position);
        }
      }
    }
  }
}

/// Check car generator differences between vanilla and modded ymap
/// Uses position as the key for identifying the same generator
pub fn check_car_generator_diff(
  vanilla_ymap: &Ymap,
  mod_ymap: &Ymap,
) -> Vec<YmapCarGeneratorDiff> {
  let vanilla = &vanilla_ymap.car_generators;
  let modded = &mod_ymap.car_generators;

  // Use position coordinates as the key for identifying the same car generator
  let vanilla_map: HashMap<(i32, i32, i32), &YmapCarGenerator> = vanilla
    .iter()
    .map(|generator| {
      let key = position_to_key(&generator.position);
      (key, generator)
    })
    .collect();

  let modded_map: HashMap<(i32, i32, i32), &YmapCarGenerator> = modded
    .iter()
    .map(|generator| {
      let key = position_to_key(&generator.position);
      (key, generator)
    })
    .collect();

  let keys: HashSet<_> = vanilla_map.keys().chain(modded_map.keys()).collect();

  keys
    .into_iter()
    .filter_map(|key| {
      let vanilla_gen = vanilla_map.get(key);
      let modded_gen = modded_map.get(key);

      match (vanilla_gen, modded_gen) {
        (Some(vanilla), Some(modded)) => {
          // Check if there are any differences
          if are_generators_equal(vanilla, modded) {
            None
          } else {
            Some(YmapCarGeneratorDiff::Modified {
              vanilla: (*vanilla).clone(),
              modded: (*modded).clone(),
            })
          }
        }
        (Some(vanilla), None) => Some(YmapCarGeneratorDiff::Removed((*vanilla).clone())),
        (None, Some(modded)) => Some(YmapCarGeneratorDiff::Added((*modded).clone())),
        (None, None) => None,
      }
    })
    .collect::<Vec<_>>()
}

/// Convert Position to integer key for HashMap (rounds to 3 decimal places)
fn position_to_key(pos: &Position) -> (i32, i32, i32) {
  (
    (pos.x * 1000.0).round() as i32,
    (pos.y * 1000.0).round() as i32,
    (pos.z * 1000.0).round() as i32,
  )
}

/// Check if two car generators are equal (ignoring position since that's the key)
fn are_generators_equal(
  a: &YmapCarGenerator,
  b: &YmapCarGenerator,
) -> bool {
  const FLOAT_EPSILON: f32 = 0.001;

  (a.orient_x - b.orient_x).abs() < FLOAT_EPSILON
    && (a.orient_y - b.orient_y).abs() < FLOAT_EPSILON
    && (a.perpendicular_length - b.perpendicular_length).abs() < FLOAT_EPSILON
    && a.car_model == b.car_model
    && a.flags == b.flags
    && a.body_color_remap_1 == b.body_color_remap_1
    && a.body_color_remap_2 == b.body_color_remap_2
    && a.body_color_remap_3 == b.body_color_remap_3
    && a.body_color_remap_4 == b.body_color_remap_4
    && a.pop_group == b.pop_group
    && a.livery == b.livery
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::format::ymap::model::{YmapBlock, YmapDistantLodLights, YmapInstancedData};
  use indexmap::IndexMap;

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
      entity_map: IndexMap::new(),
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

  fn create_test_generator(
    x: f32,
    y: f32,
    z: f32,
  ) -> YmapCarGenerator {
    YmapCarGenerator {
      position: Position {
        x,
        y,
        z,
      },
      orient_x: 1.0,
      orient_y: 0.0,
      perpendicular_length: 5.0,
      car_model: "adder".to_string(),
      flags: 0,
      body_color_remap_1: -1,
      body_color_remap_2: -1,
      body_color_remap_3: -1,
      body_color_remap_4: -1,
      pop_group: "default".to_string(),
      livery: -1,
    }
  }

  #[test]
  fn test_position_to_key() {
    let pos = Position {
      x: 1.23456,
      y: -2.34567,
      z: 3.45678,
    };
    let key = position_to_key(&pos);
    assert_eq!(key, (1235, -2346, 3457));
  }

  #[test]
  fn test_are_generators_equal() {
    let generator1 = create_test_generator(100.0, 200.0, 30.0);
    let generator2 = create_test_generator(100.0, 200.0, 30.0);
    assert!(are_generators_equal(&generator1, &generator2));

    let mut generator3 = create_test_generator(100.0, 200.0, 30.0);
    generator3.car_model = "zentorno".to_string();
    assert!(!are_generators_equal(&generator1, &generator3));
  }

  #[test]
  fn test_detect_added_generator() {
    let vanilla_ymap = create_test_ymap();
    let mut modded_ymap = create_test_ymap();

    let generator = create_test_generator(100.0, 200.0, 30.0);
    modded_ymap.car_generators.push(generator);

    let diffs = check_car_generator_diff(&vanilla_ymap, &modded_ymap);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], YmapCarGeneratorDiff::Added(_)));
  }

  #[test]
  fn test_detect_removed_generator() {
    let mut vanilla_ymap = create_test_ymap();
    let modded_ymap = create_test_ymap();

    let generator = create_test_generator(100.0, 200.0, 30.0);
    vanilla_ymap.car_generators.push(generator);

    let diffs = check_car_generator_diff(&vanilla_ymap, &modded_ymap);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], YmapCarGeneratorDiff::Removed(_)));
  }

  #[test]
  fn test_detect_modified_generator() {
    let mut vanilla_ymap = create_test_ymap();
    let mut modded_ymap = create_test_ymap();

    let generator1 = create_test_generator(100.0, 200.0, 30.0);
    vanilla_ymap.car_generators.push(generator1);

    let mut generator2 = create_test_generator(100.0, 200.0, 30.0);
    generator2.car_model = "zentorno".to_string();
    modded_ymap.car_generators.push(generator2);

    let diffs = check_car_generator_diff(&vanilla_ymap, &modded_ymap);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], YmapCarGeneratorDiff::Modified { .. }));
  }

  #[test]
  fn test_no_differences() {
    let mut vanilla_ymap = create_test_ymap();
    let mut modded_ymap = create_test_ymap();

    let generator = create_test_generator(100.0, 200.0, 30.0);
    vanilla_ymap.car_generators.push(generator.clone());
    modded_ymap.car_generators.push(generator);

    let diffs = check_car_generator_diff(&vanilla_ymap, &modded_ymap);
    assert_eq!(diffs.len(), 0);
  }
}
