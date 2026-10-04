use crate::core::format::ymap::model::{Ymap, YmapDistantLodLight};
use std::collections::{HashMap, HashSet};

#[derive(serde::Serialize, serde::Deserialize)]
pub enum YmapDistantLodLightDiff {
  Added(YmapDistantLodLight),
  Removed(YmapDistantLodLight),
  Modified {
    vanilla: YmapDistantLodLight,
    modded: YmapDistantLodLight,
  },
}

impl YmapDistantLodLightDiff {
  pub fn extract_from(
    vanilla: &Ymap,
    modded: &Ymap,
  ) -> Vec<YmapDistantLodLightDiff> {
    check_distant_lod_light_diff(vanilla, modded)
  }

  pub fn print_diffs(diffs: &Vec<YmapDistantLodLightDiff>) {
    log::info!("    Found {} distant LOD light differences", diffs.len());
    for diff in diffs {
      match diff {
        Self::Added(light) => {
          log::info!("      [Added] Distant LOD Light at: {}", light.position);
        }
        Self::Removed(light) => {
          log::info!("      [Removed] Distant LOD Light at: {}", light.position);
        }
        Self::Modified {
          vanilla,
          modded: _,
        } => {
          log::info!("      [Modified] Distant LOD Light at: {}", vanilla.position);
        }
      }
    }
  }
}

/// Check distant LOD light differences between vanilla and modded ymap
/// Uses position as the key for identifying the same light
pub fn check_distant_lod_light_diff(
  vanilla_ymap: &Ymap,
  mod_ymap: &Ymap,
) -> Vec<YmapDistantLodLightDiff> {
  let vanilla = &vanilla_ymap.distant_lod_lights.items;
  let modded = &mod_ymap.distant_lod_lights.items;

  // Use position coordinates as the key for identifying the same distant LOD light
  let vanilla_map: HashMap<(i32, i32, i32), &YmapDistantLodLight> = vanilla
    .iter()
    .map(|light| {
      let key = position_string_to_key(&light.position);
      (key, light)
    })
    .collect();

  let modded_map: HashMap<(i32, i32, i32), &YmapDistantLodLight> = modded
    .iter()
    .map(|light| {
      let key = position_string_to_key(&light.position);
      (key, light)
    })
    .collect();

  let keys: HashSet<_> = vanilla_map.keys().chain(modded_map.keys()).collect();

  keys
    .into_iter()
    .filter_map(|key| {
      let vanilla_light = vanilla_map.get(key);
      let modded_light = modded_map.get(key);

      match (vanilla_light, modded_light) {
        (Some(vanilla), Some(modded)) => {
          // Check if there are any differences
          if are_lights_equal(vanilla, modded) {
            None
          } else {
            Some(YmapDistantLodLightDiff::Modified {
              vanilla: (*vanilla).clone(),
              modded: (*modded).clone(),
            })
          }
        }
        (Some(vanilla), None) => Some(YmapDistantLodLightDiff::Removed((*vanilla).clone())),
        (None, Some(modded)) => Some(YmapDistantLodLightDiff::Added((*modded).clone())),
        (None, None) => None,
      }
    })
    .collect::<Vec<_>>()
}

/// Convert position string to integer key for HashMap (rounds to 3 decimal places)
fn position_string_to_key(pos_str: &str) -> (i32, i32, i32) {
  let coords: Vec<&str> = pos_str.split_whitespace().collect();
  if coords.len() >= 3 {
    let x = coords[0].parse::<f32>().unwrap_or(0.0);
    let y = coords[1].parse::<f32>().unwrap_or(0.0);
    let z = coords[2].parse::<f32>().unwrap_or(0.0);
    ((x * 1000.0).round() as i32, (y * 1000.0).round() as i32, (z * 1000.0).round() as i32)
  } else {
    (0, 0, 0)
  }
}

/// Check if two distant LOD lights are equal (ignoring position since that's the key)
fn are_lights_equal(
  a: &YmapDistantLodLight,
  b: &YmapDistantLodLight,
) -> bool {
  a.rgbi == b.rgbi
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::{
    common::position::Position,
    format::ymap::model::{YmapBlock, YmapDistantLodLights, YmapInstancedData},
  };
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

  fn create_test_light(
    x: f32,
    y: f32,
    z: f32,
    rgbi: &str,
  ) -> YmapDistantLodLight {
    YmapDistantLodLight {
      position: format!("{} {} {}", x, y, z),
      rgbi: rgbi.to_string(),
    }
  }

  #[test]
  fn test_position_string_to_key() {
    let pos_str = "1.23456 -2.34567 3.45678";
    let key = position_string_to_key(pos_str);
    assert_eq!(key, (1235, -2346, 3457));
  }

  #[test]
  fn test_are_lights_equal() {
    let light1 = create_test_light(0.0, 0.0, 10.0, "255");
    let light2 = create_test_light(0.0, 0.0, 10.0, "255");
    assert!(are_lights_equal(&light1, &light2));

    let light3 = create_test_light(0.0, 0.0, 10.0, "128");
    assert!(!are_lights_equal(&light1, &light3));
  }

  #[test]
  fn test_detect_added_light() {
    let vanilla_ymap = create_test_ymap();
    let mut modded_ymap = create_test_ymap();

    let light = create_test_light(100.0, 200.0, 10.0, "255");
    modded_ymap.distant_lod_lights.items.push(light);

    let diffs = check_distant_lod_light_diff(&vanilla_ymap, &modded_ymap);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], YmapDistantLodLightDiff::Added(_)));
  }

  #[test]
  fn test_detect_removed_light() {
    let mut vanilla_ymap = create_test_ymap();
    let modded_ymap = create_test_ymap();

    let light = create_test_light(100.0, 200.0, 10.0, "255");
    vanilla_ymap.distant_lod_lights.items.push(light);

    let diffs = check_distant_lod_light_diff(&vanilla_ymap, &modded_ymap);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], YmapDistantLodLightDiff::Removed(_)));
  }

  #[test]
  fn test_detect_modified_light() {
    let mut vanilla_ymap = create_test_ymap();
    let mut modded_ymap = create_test_ymap();

    let light1 = create_test_light(100.0, 200.0, 10.0, "255");
    vanilla_ymap.distant_lod_lights.items.push(light1);

    let light2 = create_test_light(100.0, 200.0, 10.0, "128");
    modded_ymap.distant_lod_lights.items.push(light2);

    let diffs = check_distant_lod_light_diff(&vanilla_ymap, &modded_ymap);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], YmapDistantLodLightDiff::Modified { .. }));
  }

  #[test]
  fn test_no_differences() {
    let mut vanilla_ymap = create_test_ymap();
    let mut modded_ymap = create_test_ymap();

    let light = create_test_light(100.0, 200.0, 10.0, "255");
    vanilla_ymap.distant_lod_lights.items.push(light.clone());
    modded_ymap.distant_lod_lights.items.push(light);

    let diffs = check_distant_lod_light_diff(&vanilla_ymap, &modded_ymap);
    assert_eq!(diffs.len(), 0);
  }
}
