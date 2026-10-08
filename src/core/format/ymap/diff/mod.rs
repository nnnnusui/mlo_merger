use std::collections::{HashMap, HashSet};

use crate::core::{
  config::blacklist::BlacklistConfig,
  format::ymap::model::{Ymap, YmapStructDiffEnum},
};
mod box_occluder;
mod car_generator;
mod distant_lod_light;
mod entity;
mod instanced_data;
mod lod_light;
#[cfg(test)]
mod merge_tests;
mod metadata;
mod occlude_model;
mod time_cycle_modifier;

use structdiff::StructDiff;

#[cfg(test)]
pub(crate) use self::instanced_data::BatchKey;
pub(crate) use self::metadata::reference_hash;
pub use self::{
  box_occluder::YmapBoxOccluderDiff,
  car_generator::YmapCarGeneratorDiff,
  distant_lod_light::YmapDistantLodLightDiff,
  entity::YmapEntityDiff,
  lod_light::YmapLodLightDiff,
  occlude_model::{YmapOccludeModelDiff, YmapOccludeModelTriangleDiff},
  time_cycle_modifier::YmapTimeCycleModifierDiff,
};
use self::{instanced_data::YmapInstancedDataDiff, metadata::YmapMetadataDiff};

/// Vanilla-relative changes used by merging and versioned cache reports.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct YmapDiff {
  pub entity_diffs: Vec<YmapEntityDiff>,
  pub box_occluder_diffs: Vec<YmapBoxOccluderDiff>,
  pub occlude_model_diffs: Vec<YmapOccludeModelDiff>,
  pub lod_light_diffs: Vec<YmapLodLightDiff>,
  pub distant_lod_light_diffs: Vec<YmapDistantLodLightDiff>,
  pub car_generator_diffs: Vec<YmapCarGeneratorDiff>,
  pub time_cycle_modifier_diffs: Vec<YmapTimeCycleModifierDiff>,
  pub content_flags: u32,
  metadata: YmapMetadataDiff,
  instanced_data: YmapInstancedDataDiff,
}

impl YmapDiff {
  pub fn extract_from(
    vanilla: &Ymap,
    modded: &Ymap,
  ) -> Self {
    check_diffs(vanilla, modded);

    let entity_diffs = YmapEntityDiff::extract_from(vanilla, modded);
    YmapEntityDiff::print_diffs(&entity_diffs);
    let box_occluder_diffs = YmapBoxOccluderDiff::extract_from(vanilla, modded);
    YmapBoxOccluderDiff::print_diffs(&box_occluder_diffs);
    let occlude_model_diffs = YmapOccludeModelDiff::extract_from(vanilla, modded);
    YmapOccludeModelDiff::print_diffs(&occlude_model_diffs);
    let lod_light_diffs = YmapLodLightDiff::extract_from(vanilla, modded);
    YmapLodLightDiff::print_diffs(&lod_light_diffs);
    let distant_lod_light_diffs = YmapDistantLodLightDiff::extract_from(vanilla, modded);
    YmapDistantLodLightDiff::print_diffs(&distant_lod_light_diffs);
    let car_generator_diffs = YmapCarGeneratorDiff::extract_from(vanilla, modded);
    YmapCarGeneratorDiff::print_diffs(&car_generator_diffs);
    let time_cycle_modifier_diffs = YmapTimeCycleModifierDiff::extract_from(vanilla, modded);
    YmapTimeCycleModifierDiff::print_diffs(&time_cycle_modifier_diffs);

    Self {
      entity_diffs,
      box_occluder_diffs,
      occlude_model_diffs,
      lod_light_diffs,
      distant_lod_light_diffs,
      car_generator_diffs,
      time_cycle_modifier_diffs,
      content_flags: modded.content_flags,
      metadata: YmapMetadataDiff::extract_from(vanilla, modded),
      instanced_data: YmapInstancedDataDiff::extract_from(
        &vanilla.instanced_data,
        &modded.instanced_data,
      ),
    }
  }

  pub fn merge(
    mut self,
    other: Self,
  ) -> Self {
    self.entity_diffs.extend(other.entity_diffs);
    let removed =
      self
        .entity_diffs
        .iter()
        .filter_map(|diff| {
          if let YmapEntityDiff::Removed(entity) = diff { Some(entity.guid) } else { None }
        })
        .collect::<HashSet<_>>();
    self.entity_diffs.retain(|diff| match diff {
      YmapEntityDiff::Removed(_) => true,
      YmapEntityDiff::Added(entity) => !removed.contains(&entity.guid),
      YmapEntityDiff::Modified {
        vanilla,
        ..
      } => !removed.contains(&vanilla.guid),
    });
    self.box_occluder_diffs.extend(other.box_occluder_diffs);
    resolve_box_occluder_removals(&mut self.box_occluder_diffs);
    self.occlude_model_diffs.extend(other.occlude_model_diffs);
    resolve_occlude_model_removals(&mut self.occlude_model_diffs);
    self.lod_light_diffs.extend(other.lod_light_diffs);
    self.distant_lod_light_diffs.extend(other.distant_lod_light_diffs);
    self.car_generator_diffs.extend(other.car_generator_diffs);
    self.time_cycle_modifier_diffs.extend(other.time_cycle_modifier_diffs);
    // Merge content_flags using bitwise OR
    self.content_flags |= other.content_flags;
    self.metadata.merge(other.metadata);
    self.instanced_data.merge(other.instanced_data);
    self
  }

  pub fn apply_to(
    self,
    vanilla: &Ymap,
    blacklist: Option<&BlacklistConfig>,
  ) -> Ymap {
    let mut modded = vanilla.clone();

    // Apply content_flags using bitwise OR with vanilla
    modded.content_flags = vanilla.content_flags | self.content_flags;
    self.metadata.apply_to(&mut modded);
    self.instanced_data.apply_to(&mut modded.instanced_data);

    // Remove duplicates from entity_diffs using Debug string as key
    let mut seen = HashSet::new();
    let unique_entity_diffs: Vec<YmapEntityDiff> = self
      .entity_diffs
      .into_iter()
      .filter(|diff| {
        let key = format!("{:?}", diff);
        seen.insert(key)
      })
      .collect();

    let entity_diffs_map: HashMap<u32, Vec<YmapEntityDiff>> =
      unique_entity_diffs.into_iter().fold(HashMap::new(), |mut map, diff| {
        let guid = match &diff {
          YmapEntityDiff::Added(e) | YmapEntityDiff::Removed(e) => e.guid,
          YmapEntityDiff::Modified {
            vanilla,
            ..
          } => vanilla.guid,
        };
        map.entry(guid).or_default().push(diff);
        map
      });

    for (guid, diffs) in entity_diffs_map {
      let diff = diffs
        .iter()
        .find(|diff| matches!(diff, YmapEntityDiff::Removed(_)))
        .unwrap_or_else(|| diffs.first().unwrap());
      if diffs.len() > 1 {
        let tails =
          diffs.iter().filter(|candidate| !std::ptr::eq(*candidate, diff)).collect::<Vec<_>>();
        log::warn!(
          "    Multiple diffs for entity GUID {}. applied: {:?} ignored: {:?}",
          guid,
          diff,
          tails
        );
      }
      match diff {
        YmapEntityDiff::Added(it) => {
          modded.entity_map.insert(it.guid, it.clone());
        }
        YmapEntityDiff::Removed(it) => {
          modded.entity_map.shift_remove(&it.guid);
        }
        YmapEntityDiff::Modified {
          vanilla: it,
          diffs,
        } => {
          let before = modded.entity_map.get_mut(&it.guid).unwrap().clone();
          let after = before.apply(diffs.to_vec());
          modded.entity_map.insert(it.guid, after);
        }
      }
    }

    let mut box_occluder_removed_vanilla_indices = Vec::new();
    for diff in self.box_occluder_diffs {
      match diff {
        YmapBoxOccluderDiff::Removed(it) => {
          if let Some(index) = vanilla.box_occluders.iter().position(|vanilla| vanilla.is_same(&it))
          {
            box_occluder_removed_vanilla_indices.push(index);
          };
          modded.box_occluders.retain(|bo| !bo.is_same(&it));
        }
        YmapBoxOccluderDiff::Added(it) => {
          modded.box_occluders.push(it.clone());
        }
        YmapBoxOccluderDiff::Modified {
          vanilla: it,
          diffs,
        } => {
          if let Some(index) = vanilla.box_occluders.iter().position(|vanilla| vanilla.is_same(&it))
            && box_occluder_removed_vanilla_indices.contains(&index)
          {
            log::warn!(
              "    skipped _ Attempting to modify a box occluder that was removed: {:?}",
              it
            );
            continue;
          };
          if let Some(index) =
            modded.box_occluders.iter().position(|modded_it| modded_it.is_same(&it))
          {
            let before = modded.box_occluders[index].clone();
            let after = before.apply(diffs);
            modded.box_occluders[index] = after;
          }
        }
      }
    }

    let mut occlude_model_removed_vanilla_indices = Vec::new();
    for diff in self.occlude_model_diffs {
      match diff {
        YmapOccludeModelDiff::Removed(it) => {
          if let Some(index) =
            vanilla.occlude_models.iter().position(|vanilla| vanilla.is_same(&it))
          {
            occlude_model_removed_vanilla_indices.push(index);
          };
          modded.occlude_models.retain(|om| !om.is_same(&it));
        }
        YmapOccludeModelDiff::Added(it) => {
          // Check if this item is blacklisted
          if let Some(blacklist_config) = blacklist
            && blacklist_config.is_occlude_model_blacklisted(&it.bmin, &it.bmax)
          {
            log::info!("      [Blacklisted] Occlude Model: bmin={:?}, bmax={:?}", it.bmin, it.bmax);
            continue;
          }
          modded.occlude_models.push(it.clone());
        }
        YmapOccludeModelDiff::Modified {
          vanilla: it,
          diffs,
          triangle_diffs,
        } => {
          if let Some(index) =
            vanilla.occlude_models.iter().position(|vanilla| vanilla.is_same(&it))
            && occlude_model_removed_vanilla_indices.contains(&index)
          {
            log::warn!(
              "    skipped _ Attempting to modify a occlude model that was removed: {:?}",
              it
            );
            continue;
          };
          if let Some(index) =
            modded.occlude_models.iter().position(|modded_it| modded_it.is_same(&it))
          {
            let before = modded.occlude_models[index].clone();
            let mut after = before.apply(diffs);
            for diff in triangle_diffs {
              match diff {
                YmapOccludeModelTriangleDiff::Added(it) => {
                  let mut triangles = after.triangles.clone();
                  triangles.push(it.clone());
                  after.triangles = triangles;
                }
                YmapOccludeModelTriangleDiff::Removed(it) => {
                  let mut triangles = after.triangles.clone();
                  triangles.retain(|t| t != &it);
                  after.triangles = triangles;
                }
              }
            }
            modded.occlude_models[index] = after;
          }
        }
      }
    }

    for diff in self.lod_light_diffs {
      match diff {
        YmapLodLightDiff::Removed(it) => {
          modded.lod_lights.retain(|light| {
            // Compare by direction (rounded to 3 decimal places)
            let key1 = (
              (light.direction.x * 1000.0).round() as i32,
              (light.direction.y * 1000.0).round() as i32,
              (light.direction.z * 1000.0).round() as i32,
            );
            let key2 = (
              (it.direction.x * 1000.0).round() as i32,
              (it.direction.y * 1000.0).round() as i32,
              (it.direction.z * 1000.0).round() as i32,
            );
            key1 != key2
          });
        }
        YmapLodLightDiff::Added(it) => {
          modded.lod_lights.push(it.clone());
        }
        YmapLodLightDiff::Modified {
          vanilla: _,
          modded: new_light,
        } => {
          // Find and replace the light with the same direction
          if let Some(index) = modded.lod_lights.iter().position(|light| {
            let key1 = (
              (light.direction.x * 1000.0).round() as i32,
              (light.direction.y * 1000.0).round() as i32,
              (light.direction.z * 1000.0).round() as i32,
            );
            let key2 = (
              (new_light.direction.x * 1000.0).round() as i32,
              (new_light.direction.y * 1000.0).round() as i32,
              (new_light.direction.z * 1000.0).round() as i32,
            );
            key1 == key2
          }) {
            modded.lod_lights[index] = new_light;
          }
        }
      }
    }

    for diff in self.distant_lod_light_diffs {
      match diff {
        YmapDistantLodLightDiff::Removed(it) => {
          modded.distant_lod_lights.items.retain(|light| {
            // Compare by position string (rounded to 3 decimal places)
            let coords1: Vec<&str> = light.position.split_whitespace().collect();
            let coords2: Vec<&str> = it.position.split_whitespace().collect();
            if coords1.len() >= 3 && coords2.len() >= 3 {
              let key1 = (
                (coords1[0].parse::<f32>().unwrap_or(0.0) * 1000.0).round() as i32,
                (coords1[1].parse::<f32>().unwrap_or(0.0) * 1000.0).round() as i32,
                (coords1[2].parse::<f32>().unwrap_or(0.0) * 1000.0).round() as i32,
              );
              let key2 = (
                (coords2[0].parse::<f32>().unwrap_or(0.0) * 1000.0).round() as i32,
                (coords2[1].parse::<f32>().unwrap_or(0.0) * 1000.0).round() as i32,
                (coords2[2].parse::<f32>().unwrap_or(0.0) * 1000.0).round() as i32,
              );
              key1 != key2
            } else {
              true
            }
          });
        }
        YmapDistantLodLightDiff::Added(it) => {
          modded.distant_lod_lights.items.push(it.clone());
        }
        YmapDistantLodLightDiff::Modified {
          vanilla: _,
          modded: new_light,
        } => {
          // Find and replace the light with the same position
          if let Some(index) = modded.distant_lod_lights.items.iter().position(|light| {
            let coords1: Vec<&str> = light.position.split_whitespace().collect();
            let coords2: Vec<&str> = new_light.position.split_whitespace().collect();
            if coords1.len() >= 3 && coords2.len() >= 3 {
              let key1 = (
                (coords1[0].parse::<f32>().unwrap_or(0.0) * 1000.0).round() as i32,
                (coords1[1].parse::<f32>().unwrap_or(0.0) * 1000.0).round() as i32,
                (coords1[2].parse::<f32>().unwrap_or(0.0) * 1000.0).round() as i32,
              );
              let key2 = (
                (coords2[0].parse::<f32>().unwrap_or(0.0) * 1000.0).round() as i32,
                (coords2[1].parse::<f32>().unwrap_or(0.0) * 1000.0).round() as i32,
                (coords2[2].parse::<f32>().unwrap_or(0.0) * 1000.0).round() as i32,
              );
              key1 == key2
            } else {
              false
            }
          }) {
            modded.distant_lod_lights.items[index] = new_light;
          }
        }
      }
    }

    for diff in self.car_generator_diffs {
      match diff {
        YmapCarGeneratorDiff::Removed(it) => {
          modded.car_generators.retain(|generator| {
            // Compare by position (rounded to 3 decimal places)
            let key1 = (
              (generator.position.x * 1000.0).round() as i32,
              (generator.position.y * 1000.0).round() as i32,
              (generator.position.z * 1000.0).round() as i32,
            );
            let key2 = (
              (it.position.x * 1000.0).round() as i32,
              (it.position.y * 1000.0).round() as i32,
              (it.position.z * 1000.0).round() as i32,
            );
            key1 != key2
          });
        }
        YmapCarGeneratorDiff::Added(it) => {
          modded.car_generators.push(it.clone());
        }
        YmapCarGeneratorDiff::Modified {
          vanilla: _,
          modded: new_generator,
        } => {
          // Find and replace the generator with the same position
          if let Some(index) = modded.car_generators.iter().position(|generator| {
            let key1 = (
              (generator.position.x * 1000.0).round() as i32,
              (generator.position.y * 1000.0).round() as i32,
              (generator.position.z * 1000.0).round() as i32,
            );
            let key2 = (
              (new_generator.position.x * 1000.0).round() as i32,
              (new_generator.position.y * 1000.0).round() as i32,
              (new_generator.position.z * 1000.0).round() as i32,
            );
            key1 == key2
          }) {
            modded.car_generators[index] = new_generator;
          }
        }
      }
    }

    for diff in self.time_cycle_modifier_diffs {
      match diff {
        YmapTimeCycleModifierDiff::Removed(it) => {
          modded.time_cycle_modifiers.retain(|modifier| {
            // Compare by min_extents or max_extents
            modifier.min_extents != it.min_extents && modifier.max_extents != it.max_extents
          });
        }
        YmapTimeCycleModifierDiff::Added(it) => {
          modded.time_cycle_modifiers.push(it.clone());
        }
        YmapTimeCycleModifierDiff::Modified {
          vanilla,
          diffs,
        } => {
          // Find and apply modifications to the modifier
          if let Some(index) = modded.time_cycle_modifiers.iter().position(|modifier| {
            modifier.min_extents == vanilla.min_extents
              || modifier.max_extents == vanilla.max_extents
          }) {
            let before = modded.time_cycle_modifiers[index].clone();
            let after = before.apply(diffs);
            modded.time_cycle_modifiers[index] = after;
          }
        }
      }
    }

    modded
  }
}

fn resolve_box_occluder_removals(diffs: &mut Vec<YmapBoxOccluderDiff>) {
  let removed = diffs
    .iter()
    .filter_map(|diff| match diff {
      YmapBoxOccluderDiff::Removed(item) => Some(item.clone()),
      _ => None,
    })
    .collect::<Vec<_>>();
  diffs.retain(|diff| match diff {
    YmapBoxOccluderDiff::Removed(_) => true,
    YmapBoxOccluderDiff::Added(item) => !removed.iter().any(|removed| removed.is_same(item)),
    YmapBoxOccluderDiff::Modified {
      vanilla,
      ..
    } => !removed.iter().any(|removed| removed.is_same(vanilla)),
  });
}

fn resolve_occlude_model_removals(diffs: &mut Vec<YmapOccludeModelDiff>) {
  let removed_models = diffs
    .iter()
    .filter_map(|diff| match diff {
      YmapOccludeModelDiff::Removed(model) => Some(model.clone()),
      _ => None,
    })
    .collect::<Vec<_>>();
  diffs.retain(|diff| match diff {
    YmapOccludeModelDiff::Removed(_) => true,
    YmapOccludeModelDiff::Added(model) => {
      !removed_models.iter().any(|removed| removed.is_same(model))
    }
    YmapOccludeModelDiff::Modified {
      vanilla,
      ..
    } => !removed_models.iter().any(|removed| removed.is_same(vanilla)),
  });

  let removed_triangles = diffs
    .iter()
    .filter_map(|diff| match diff {
      YmapOccludeModelDiff::Modified {
        vanilla,
        triangle_diffs,
        ..
      } => {
        let triangles = triangle_diffs
          .iter()
          .filter_map(|diff| match diff {
            YmapOccludeModelTriangleDiff::Removed(triangle) => Some(triangle.clone()),
            YmapOccludeModelTriangleDiff::Added(_) => None,
          })
          .collect::<Vec<_>>();
        (!triangles.is_empty()).then(|| (vanilla.clone(), triangles))
      }
      _ => None,
    })
    .collect::<Vec<_>>();
  for diff in diffs {
    if let YmapOccludeModelDiff::Modified {
      vanilla,
      triangle_diffs,
      ..
    } = diff
    {
      triangle_diffs.retain(|diff| match diff {
        YmapOccludeModelTriangleDiff::Removed(_) => true,
        YmapOccludeModelTriangleDiff::Added(triangle) => !removed_triangles
          .iter()
          .filter(|(model, _)| model.is_same(vanilla))
          .any(|(_, removed)| removed.contains(triangle)),
      });
    }
  }
}

#[rustfmt::skip]
fn check_diffs(
    vanilla: &Ymap,
    modded: &Ymap,
) {
  let diffs = vanilla.diff(modded);
  for diff in &diffs {
    match diff {
      YmapStructDiffEnum::name(_) => log::info!("    skip changes: <name /> _ {} -> {}", vanilla.name, modded.name),
      YmapStructDiffEnum::flags(_) => log::info!("    skip changes: <contentFlags /> _ {:?} -> {:?}", vanilla.content_flags, modded.content_flags),
      YmapStructDiffEnum::content_flags(_) => {}
      YmapStructDiffEnum::streaming_extents_max(_) => log::info!("    skip changes: <streamingExtentsMax />"),
      YmapStructDiffEnum::streaming_extents_min(_) => log::info!("    skip changes: <streamingExtentsMin />"),
      YmapStructDiffEnum::entities_extents_max(_) => log::info!("    skip changes: <entitiesExtentsMax />"),
      YmapStructDiffEnum::entities_extents_min(_) => log::info!("    skip changes: <entitiesExtentsMin />"),
      YmapStructDiffEnum::entity_map(_) => {}
      YmapStructDiffEnum::box_occluders(_) => {}
      YmapStructDiffEnum::occlude_models(_) => {}
      YmapStructDiffEnum::time_cycle_modifiers(_) => {}
      YmapStructDiffEnum::car_generators(_) => {}
      YmapStructDiffEnum::lod_lights(_) => {}
      YmapStructDiffEnum::distant_lod_lights(_) => {}
      YmapStructDiffEnum::block(_) => log::info!("    skip changes: <block/>"),
      YmapStructDiffEnum::parent(_) | YmapStructDiffEnum::physics_dictionaries(_) => {}
      YmapStructDiffEnum::instanced_data(_) => {}
      it => log::warn!("    skip unsupported changes: {:?}", it),
    }
  }
}
