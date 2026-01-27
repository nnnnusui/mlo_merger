use crate::core::format::ymap::model::{Ymap, YmapEntity, ymap_entity::YmapEntityStructDiffEnum};
use structdiff::StructDiff;

pub enum YmapEntityDiff {
  Added(YmapEntity),
  Removed(YmapEntity),
  Modified {
    vanilla: YmapEntity,
    diffs: Vec<YmapEntityStructDiffEnum>,
  },
}

pub fn check_entity_diff(
  vanilla_ymap: &Ymap,
  mod_ymap: &Ymap,
) -> Vec<YmapEntityDiff> {
  let vanilla_entities_map = &vanilla_ymap.entity_map;
  let mod_entities_map = &mod_ymap.entity_map;
  let keys = vanilla_entities_map
    .keys()
    .chain(mod_entities_map.keys())
    .collect::<std::collections::HashSet<_>>();

  keys
    .into_iter()
    .filter_map(|key| {
      let vanilla_entity = vanilla_entities_map.get(key);
      let mod_entity = mod_entities_map.get(key);

      match (vanilla_entity, mod_entity) {
        (Some(vanilla), Some(modded)) => {
          let diffs = vanilla.diff(modded);
          if diffs.is_empty() {
            None
          } else {
            Some(YmapEntityDiff::Modified {
              vanilla: vanilla.clone(),
              diffs,
            })
          }
        }
        (Some(vanilla), None) => Some(YmapEntityDiff::Removed(vanilla.clone())),
        (None, Some(modded)) => Some(YmapEntityDiff::Added(modded.clone())),
        (None, None) => None,
      }
    })
    .collect::<Vec<_>>()
}
