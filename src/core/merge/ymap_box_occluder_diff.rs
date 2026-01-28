use crate::{
  core::{
    common::position::Position,
    format::ymap::model::{Ymap, YmapBoxOccluder, YmapBoxOccluderStructDiffEnum},
  },
  return_early,
};
use std::collections::{HashMap, HashSet};
use structdiff::StructDiff;

pub enum YmapBoxOccluderDiff {
  Added(YmapBoxOccluder),
  Removed(YmapBoxOccluder),
  Modified {
    vanilla: YmapBoxOccluder,
    diffs: Vec<YmapBoxOccluderStructDiffEnum>,
  },
}

impl YmapBoxOccluderDiff {
  pub fn extract_from(
    vanilla: &Ymap,
    modded: &Ymap,
  ) -> Vec<YmapBoxOccluderDiff> {
    check_box_occluder_diff(vanilla, modded)
  }

  pub fn print_diffs(diffs: &Vec<YmapBoxOccluderDiff>) {
    log::info!("    Found {} box occluder differences", diffs.len());
    for diff in diffs {
      match diff {
        Self::Added(b) => {
          log::info!(
            "      [Added] Box Occluder: {:?}",
            Position {
              x: b.i_center_x as f32,
              y: b.i_center_y as f32,
              z: b.i_center_z as f32,
            }
          );
        }
        Self::Removed(b) => {
          log::info!(
            "      [Removed] Box Occluder: {:?}",
            Position {
              x: b.i_center_x as f32,
              y: b.i_center_y as f32,
              z: b.i_center_z as f32,
            }
          );
        }
        Self::Modified {
          vanilla: _,
          diffs,
        } => {
          log::info!("      [Modified] Diffs: {:?}", diffs);
        }
      }
    }
  }
}

pub fn check_box_occluder_diff(
  vanilla_ymap: &Ymap,
  mod_ymap: &Ymap,
) -> Vec<YmapBoxOccluderDiff> {
  let vanilla = &vanilla_ymap.box_occluders;
  let modded = &mod_ymap.box_occluders;

  // Use center coordinates (x, y, z) as the key for identifying the same box occluder
  let vanilla_map: HashMap<(i32, i32, i32), &YmapBoxOccluder> = vanilla
    .iter()
    .map(|occluder| ((occluder.i_center_x, occluder.i_center_y, occluder.i_center_z), occluder))
    .collect();

  let modded_map: HashMap<(i32, i32, i32), &YmapBoxOccluder> = modded
    .iter()
    .map(|occluder| ((occluder.i_center_x, occluder.i_center_y, occluder.i_center_z), occluder))
    .collect();

  let keys: HashSet<_> = vanilla_map.keys().chain(modded_map.keys()).collect();

  keys
    .into_iter()
    .filter_map(|key| {
      let vanilla_occluder = vanilla_map.get(key);
      let modded_occluder = modded_map.get(key);

      match (vanilla_occluder, modded_occluder) {
        (Some(vanilla), Some(modded)) => {
          let diffs: Vec<YmapBoxOccluderStructDiffEnum> = vanilla
            .diff(modded)
            .into_iter()
            .filter(|diff| match diff {
              YmapBoxOccluderStructDiffEnum::i_cos_z(it) => 1 < (vanilla.i_cos_z - it).abs(),
              YmapBoxOccluderStructDiffEnum::i_sin_z(it) => 1 < (vanilla.i_sin_z - it).abs(),
              _ => true,
            })
            .collect();

          return_early!(if (diffs.is_empty()) return None);
          // return_early!(if (diffs.is_empty()) return None);
          Some(YmapBoxOccluderDiff::Modified {
            vanilla: (*vanilla).clone(),
            diffs,
          })
        }
        (Some(vanilla), None) => Some(YmapBoxOccluderDiff::Removed((*vanilla).clone())),
        (None, Some(modded)) => Some(YmapBoxOccluderDiff::Added((*modded).clone())),
        (None, None) => None,
      }
    })
    .collect::<Vec<_>>()
}
