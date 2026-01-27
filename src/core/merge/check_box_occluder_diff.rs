use crate::{
  core::format::ymap::model::{Ymap, YmapBoxOccluder},
  return_early,
};
use std::collections::{HashMap, HashSet};

pub enum YmapBoxOccluderDiff {
  Added(YmapBoxOccluder),
  Removed(YmapBoxOccluder),
  Modified {
    vanilla: YmapBoxOccluder,
    modded: YmapBoxOccluder,
  },
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
          return_early!(if (vanilla == modded) return None);
          return_early!(if ((vanilla.i_sin_z - modded.i_sin_z).abs() <= 1) return None);
          return_early!(if ((vanilla.i_cos_z - modded.i_cos_z).abs() <= 1) return None);

          Some(YmapBoxOccluderDiff::Modified {
            vanilla: (*vanilla).clone(),
            modded: (*modded).clone(),
          })
        }
        (Some(vanilla), None) => Some(YmapBoxOccluderDiff::Removed((*vanilla).clone())),
        (None, Some(modded)) => Some(YmapBoxOccluderDiff::Added((*modded).clone())),
        (None, None) => None,
      }
    })
    .collect::<Vec<_>>()
}
