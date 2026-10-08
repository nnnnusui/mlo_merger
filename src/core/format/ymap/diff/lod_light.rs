use crate::core::{
  common::position::Position,
  format::ymap::model::{Ymap, YmapLodLight},
};
use std::collections::{HashMap, HashSet};

#[derive(serde::Serialize, serde::Deserialize)]
pub enum YmapLodLightDiff {
  Added(YmapLodLight),
  Removed(YmapLodLight),
  Modified {
    vanilla: YmapLodLight,
    modded: YmapLodLight,
  },
}

impl YmapLodLightDiff {
  pub fn extract_from(
    vanilla: &Ymap,
    modded: &Ymap,
  ) -> Vec<YmapLodLightDiff> {
    check_lod_light_diff(vanilla, modded)
  }

  pub fn print_diffs(diffs: &Vec<YmapLodLightDiff>) {
    log::info!("    Found {} LOD light differences", diffs.len());
    for diff in diffs {
      match diff {
        Self::Added(light) => {
          log::info!("      [Added] LOD Light at: {:?}", light.direction);
        }
        Self::Removed(light) => {
          log::info!("      [Removed] LOD Light at: {:?}", light.direction);
        }
        Self::Modified {
          vanilla,
          modded: _,
        } => {
          log::info!("      [Modified] LOD Light at: {:?}", vanilla.direction);
        }
      }
    }
  }
}

/// Check LOD light differences between vanilla and modded ymap
/// Uses direction (Position) as the key for identifying the same light
pub fn check_lod_light_diff(
  vanilla_ymap: &Ymap,
  mod_ymap: &Ymap,
) -> Vec<YmapLodLightDiff> {
  let vanilla = &vanilla_ymap.lod_lights;
  let modded = &mod_ymap.lod_lights;

  // Use direction coordinates (x, y, z) as the key for identifying the same LOD light
  // We need to handle floating point comparison, so we'll use a rounded representation
  let vanilla_map: HashMap<(i32, i32, i32), &YmapLodLight> = vanilla
    .iter()
    .map(|light| {
      let key = position_to_key(&light.direction);
      (key, light)
    })
    .collect();

  let modded_map: HashMap<(i32, i32, i32), &YmapLodLight> = modded
    .iter()
    .map(|light| {
      let key = position_to_key(&light.direction);
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
            Some(YmapLodLightDiff::Modified {
              vanilla: (*vanilla).clone(),
              modded: (*modded).clone(),
            })
          }
        }
        (Some(vanilla), None) => Some(YmapLodLightDiff::Removed((*vanilla).clone())),
        (None, Some(modded)) => Some(YmapLodLightDiff::Added((*modded).clone())),
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

/// Check if two LOD lights are equal (ignoring direction since that's the key)
fn are_lights_equal(
  a: &YmapLodLight,
  b: &YmapLodLight,
) -> bool {
  let tolerance = crate::core::config::matching::current().ymap;

  (a.falloff - b.falloff).abs() < tolerance
    && a.falloff_exponent == b.falloff_exponent
    && a.time_and_state_flags == b.time_and_state_flags
    && a.hash == b.hash
    && a.cone_inner_angle == b.cone_inner_angle
    && a.cone_outer_angle_or_cap_ext == b.cone_outer_angle_or_cap_ext
    && (a.corona_intensity - b.corona_intensity).abs() < tolerance
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

  fn create_test_light(
    x: f32,
    y: f32,
    z: f32,
  ) -> YmapLodLight {
    YmapLodLight {
      direction: Position {
        x,
        y,
        z,
      },
      falloff: 10.0,
      falloff_exponent: 128.0,
      time_and_state_flags: "150994943".to_string(),
      hash: "10038078".to_string(),
      cone_inner_angle: 70,
      cone_outer_angle_or_cap_ext: 113,
      corona_intensity: 0.0,
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
  fn test_are_lights_equal() {
    let light1 = create_test_light(0.0, 0.0, -1.0);
    let light2 = create_test_light(0.0, 0.0, -1.0);
    assert!(are_lights_equal(&light1, &light2));

    let mut light3 = create_test_light(0.0, 0.0, -1.0);
    light3.falloff = 15.0;
    assert!(!are_lights_equal(&light1, &light3));
  }

  #[test]
  fn test_detect_added_light() {
    let vanilla_ymap = create_test_ymap();
    let mut modded_ymap = create_test_ymap();

    let light = create_test_light(0.0, 0.0, -1.0);
    modded_ymap.lod_lights.push(light);

    let diffs = check_lod_light_diff(&vanilla_ymap, &modded_ymap);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], YmapLodLightDiff::Added(_)));
  }

  #[test]
  fn configured_ymap_tolerance_controls_light_scalars() {
    use crate::core::config::matching::{MatchTolerances, with_tolerances};
    let original = create_test_light(0.0, 0.0, -1.0);
    let mut changed = original.clone();
    changed.falloff += 0.02;
    assert!(!are_lights_equal(&original, &changed));
    with_tolerances(
      MatchTolerances {
        ymap: 0.05,
        ..Default::default()
      },
      || {
        assert!(are_lights_equal(&original, &changed));
        changed.hash = "different_hash".into();
        assert!(!are_lights_equal(&original, &changed));
      },
    );
  }

  #[test]
  fn test_detect_removed_light() {
    let mut vanilla_ymap = create_test_ymap();
    let modded_ymap = create_test_ymap();

    let light = create_test_light(0.0, 0.0, -1.0);
    vanilla_ymap.lod_lights.push(light);

    let diffs = check_lod_light_diff(&vanilla_ymap, &modded_ymap);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], YmapLodLightDiff::Removed(_)));
  }

  #[test]
  fn test_detect_modified_light() {
    let mut vanilla_ymap = create_test_ymap();
    let mut modded_ymap = create_test_ymap();

    let light1 = create_test_light(0.0, 0.0, -1.0);
    vanilla_ymap.lod_lights.push(light1);

    let mut light2 = create_test_light(0.0, 0.0, -1.0);
    light2.falloff = 15.0;
    modded_ymap.lod_lights.push(light2);

    let diffs = check_lod_light_diff(&vanilla_ymap, &modded_ymap);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], YmapLodLightDiff::Modified { .. }));
  }

  #[test]
  fn test_no_differences() {
    let mut vanilla_ymap = create_test_ymap();
    let mut modded_ymap = create_test_ymap();

    let light = create_test_light(0.0, 0.0, -1.0);
    vanilla_ymap.lod_lights.push(light.clone());
    modded_ymap.lod_lights.push(light);

    let diffs = check_lod_light_diff(&vanilla_ymap, &modded_ymap);
    assert_eq!(diffs.len(), 0);
  }
}
