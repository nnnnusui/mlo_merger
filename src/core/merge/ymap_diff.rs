use std::collections::HashMap;

use crate::core::{
  format::ymap::model::{Ymap, YmapStructDiffEnum},
  merge::{
    ymap_box_occluder_diff::YmapBoxOccluderDiff,
    ymap_entitiy_diff::YmapEntityDiff,
    ymap_occlude_model_diff::{YmapOccludeModelDiff, YmapOccludeModelTriangleDiff},
  },
};
use structdiff::StructDiff;

pub struct YmapDiff {
  pub entity_diffs: Vec<YmapEntityDiff>,
  pub box_occluder_diffs: Vec<YmapBoxOccluderDiff>,
  pub occlude_model_diffs: Vec<YmapOccludeModelDiff>,
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

    Self {
      entity_diffs,
      box_occluder_diffs,
      occlude_model_diffs,
    }
  }

  pub fn merge(
    mut self,
    other: Self,
  ) -> Self {
    self.entity_diffs.extend(other.entity_diffs);
    self.box_occluder_diffs.extend(other.box_occluder_diffs);
    self.occlude_model_diffs.extend(other.occlude_model_diffs);
    self
  }

  pub fn apply_to(
    self,
    vanilla: &Ymap,
  ) -> Ymap {
    let mut modded = vanilla.clone();

    let entity_diffs_map: HashMap<u32, Vec<YmapEntityDiff>> =
      self.entity_diffs.into_iter().fold(HashMap::new(), |mut map, diff| {
        let guid = match &diff {
          YmapEntityDiff::Removed(e) => e.guid,
          YmapEntityDiff::Added(e) => e.guid,
          YmapEntityDiff::Modified {
            vanilla,
            ..
          } => vanilla.guid,
        };
        map.entry(guid).or_default().push(diff);
        map
      });

    for (guid, diffs) in entity_diffs_map {
      let diff = diffs.first().unwrap();
      if diffs.len() > 1 {
        let tails = diffs.iter().skip(1).collect::<Vec<_>>();
        log::warn!("    Multiple diffs for entity GUID {}. ignored: {:?}", guid, tails);
      }
      match diff {
        YmapEntityDiff::Removed(it) => {
          modded.entity_map.shift_remove(&it.guid);
        }
        YmapEntityDiff::Added(it) => {
          modded.entity_map.insert(it.guid, it.clone());
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

    for diff in self.box_occluder_diffs {
      match diff {
        YmapBoxOccluderDiff::Removed(it) => {
          modded.box_occluders.retain(|bo| bo != &it);
        }
        YmapBoxOccluderDiff::Added(it) => {
          modded.box_occluders.push(it.clone());
        }
        YmapBoxOccluderDiff::Modified {
          vanilla,
          diffs,
        } => {
          if let Some(index) = modded.box_occluders.iter().position(|it| it == &vanilla) {
            let before = modded.box_occluders[index].clone();
            let after = before.apply(diffs);
            modded.box_occluders[index] = after;
          }
        }
      }
    }

    for diff in self.occlude_model_diffs {
      match diff {
        YmapOccludeModelDiff::Removed(it) => {
          modded.occlude_models.retain(|om| om != &it);
        }
        YmapOccludeModelDiff::Added(it) => {
          modded.occlude_models.push(it.clone());
        }
        YmapOccludeModelDiff::Modified {
          vanilla,
          diffs,
          triangle_diffs,
        } => {
          if let Some(index) = modded.occlude_models.iter().position(|it| it == &vanilla) {
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

    modded
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
      YmapStructDiffEnum::entity_map(_) => {}
      YmapStructDiffEnum::box_occluders(_) => {}
      YmapStructDiffEnum::occlude_models(_) => {}
      YmapStructDiffEnum::name(_) => log::info!("    [info] skip changes: <name /> _ {} -> {}", vanilla.name, modded.name),
      YmapStructDiffEnum::block(_) => log::info!("    [info] skip changes: <block/>"),
      YmapStructDiffEnum::streaming_extents_max(_) => log::info!("    [info] skip changes: <streamingExtentsMax />"),
      YmapStructDiffEnum::streaming_extents_min(_) => log::info!("    [info] skip changes: <streamingExtentsMin />"),
      YmapStructDiffEnum::entities_extents_max(_) => log::info!("    [info] skip changes: <entitiesExtentsMax />"),
      YmapStructDiffEnum::entities_extents_min(_) => log::info!("    [info] skip changes: <entitiesExtentsMin />"),
      YmapStructDiffEnum::flags(_) => log::info!("    [info] skip changes: <contentFlags /> _ {:?} -> {:?}", vanilla.content_flags, modded.content_flags),
      it => log::warn!("    [warning] skip unsupported changes: {:?}", it),
    }
  }
}
