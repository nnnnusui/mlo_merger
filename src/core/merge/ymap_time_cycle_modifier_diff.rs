use crate::{
  continue_early,
  core::format::ymap::model::{
    Ymap, YmapTimeCycleModifier, ymap_time_cycle_modifier::YmapTimeCycleModifierStructDiffEnum,
  },
};
use structdiff::StructDiff;

#[derive(serde::Serialize, serde::Deserialize)]
pub enum YmapTimeCycleModifierDiff {
  Added(YmapTimeCycleModifier),
  Removed(YmapTimeCycleModifier),
  Modified {
    vanilla: YmapTimeCycleModifier,
    diffs: Vec<YmapTimeCycleModifierStructDiffEnum>,
  },
}

impl YmapTimeCycleModifierDiff {
  pub fn extract_from(
    vanilla: &Ymap,
    modded: &Ymap,
  ) -> Vec<YmapTimeCycleModifierDiff> {
    check_time_cycle_modifier_diff(vanilla, modded)
  }

  pub fn print_diffs(diffs: &Vec<YmapTimeCycleModifierDiff>) {
    log::info!("    Found {} time cycle modifier differences", diffs.len());
    for diff in diffs {
      match diff {
        Self::Added(modifier) => {
          log::info!(
            "      [Added] Time Cycle Modifier: name={}, min={:?}, max={:?}",
            modifier.name,
            modifier.min_extents,
            modifier.max_extents
          );
        }
        Self::Removed(modifier) => {
          log::info!(
            "      [Removed] Time Cycle Modifier: name={}, min={:?}, max={:?}",
            modifier.name,
            modifier.min_extents,
            modifier.max_extents
          );
        }
        Self::Modified {
          vanilla,
          diffs,
        } => {
          log::info!(
            "      [Modified] Time Cycle Modifier: name={}, min={:?}, max={:?}",
            vanilla.name,
            vanilla.min_extents,
            vanilla.max_extents
          );
          log::info!("        Diffs: {:?}", diffs);
        }
      }
    }
  }
}

pub fn check_time_cycle_modifier_diff(
  vanilla_ymap: &Ymap,
  mod_ymap: &Ymap,
) -> Vec<YmapTimeCycleModifierDiff> {
  let vanilla = &vanilla_ymap.time_cycle_modifiers;
  let modified = &mod_ymap.time_cycle_modifiers;

  let mut diffs = Vec::new();

  // Check for removed and modified items
  for vanilla_item in vanilla {
    if let Some(mod_item) = modified.iter().find(|m| is_same_time_cycle_modifier(vanilla_item, m)) {
      // Item exists in both, check if modified
      let item_diffs = vanilla_item.diff(mod_item);
      continue_early!(if (item_diffs.is_empty()) continue);

      diffs.push(YmapTimeCycleModifierDiff::Modified {
        vanilla: vanilla_item.clone(),
        diffs: item_diffs,
      });
    } else {
      // Item only in vanilla
      diffs.push(YmapTimeCycleModifierDiff::Removed(vanilla_item.clone()));
    }
  }

  // Check for added items
  for mod_item in modified {
    if !vanilla.iter().any(|v| is_same_time_cycle_modifier(v, mod_item)) {
      diffs.push(YmapTimeCycleModifierDiff::Added(mod_item.clone()));
    }
  }

  diffs
}

fn is_same_time_cycle_modifier(
  a: &YmapTimeCycleModifier,
  b: &YmapTimeCycleModifier,
) -> bool {
  a.min_extents == b.min_extents || a.max_extents == b.max_extents
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

  fn create_test_modifier(
    name: &str,
    min_x: f32,
    min_y: f32,
    min_z: f32,
  ) -> YmapTimeCycleModifier {
    YmapTimeCycleModifier {
      name: name.to_string(),
      min_extents: Position {
        x: min_x,
        y: min_y,
        z: min_z,
      },
      max_extents: Position {
        x: min_x + 10.0,
        y: min_y + 10.0,
        z: min_z + 5.0,
      },
      percentage: 50.0,
      range: 5.0,
      start_hour: 0,
      end_hour: 23,
    }
  }

  #[test]
  fn test_is_same_time_cycle_modifier() {
    let modifier1 = create_test_modifier("test", 100.0, 200.0, 30.0);
    let modifier2 = create_test_modifier("test", 100.0, 200.0, 30.0);
    assert!(is_same_time_cycle_modifier(&modifier1, &modifier2));

    let modifier3 = create_test_modifier("different", 150.0, 250.0, 35.0);
    assert!(!is_same_time_cycle_modifier(&modifier1, &modifier3));
  }

  #[test]
  fn test_detect_added_modifier() {
    let vanilla_ymap = create_test_ymap();
    let mut modded_ymap = create_test_ymap();

    let modifier = create_test_modifier("noambientmult", 100.0, 200.0, 30.0);
    modded_ymap.time_cycle_modifiers.push(modifier);

    let diffs = check_time_cycle_modifier_diff(&vanilla_ymap, &modded_ymap);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], YmapTimeCycleModifierDiff::Added(_)));
  }

  #[test]
  fn test_detect_removed_modifier() {
    let mut vanilla_ymap = create_test_ymap();
    let modded_ymap = create_test_ymap();

    let modifier = create_test_modifier("noambientmult", 100.0, 200.0, 30.0);
    vanilla_ymap.time_cycle_modifiers.push(modifier);

    let diffs = check_time_cycle_modifier_diff(&vanilla_ymap, &modded_ymap);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], YmapTimeCycleModifierDiff::Removed(_)));
  }

  #[test]
  fn test_detect_modified_modifier() {
    let mut vanilla_ymap = create_test_ymap();
    let mut modded_ymap = create_test_ymap();

    let modifier1 = create_test_modifier("noambientmult", 100.0, 200.0, 30.0);
    vanilla_ymap.time_cycle_modifiers.push(modifier1);

    let mut modifier2 = create_test_modifier("noambientmult", 100.0, 200.0, 30.0);
    modifier2.percentage = 75.0;
    modded_ymap.time_cycle_modifiers.push(modifier2);

    let diffs = check_time_cycle_modifier_diff(&vanilla_ymap, &modded_ymap);
    assert_eq!(diffs.len(), 1);
    assert!(matches!(diffs[0], YmapTimeCycleModifierDiff::Modified { .. }));
  }

  #[test]
  fn test_no_differences() {
    let mut vanilla_ymap = create_test_ymap();
    let mut modded_ymap = create_test_ymap();

    let modifier = create_test_modifier("noambientmult", 100.0, 200.0, 30.0);
    vanilla_ymap.time_cycle_modifiers.push(modifier.clone());
    modded_ymap.time_cycle_modifiers.push(modifier);

    let diffs = check_time_cycle_modifier_diff(&vanilla_ymap, &modded_ymap);
    assert_eq!(diffs.len(), 0);
  }
}
