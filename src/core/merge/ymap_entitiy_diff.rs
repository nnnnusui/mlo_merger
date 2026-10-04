use crate::{
  core::format::ymap::model::{Ymap, YmapEntity, ymap_entity::YmapEntityStructDiffEnum},
  return_early,
};
use structdiff::StructDiff;

#[derive(Debug)]
pub enum YmapEntityDiff {
  Added(YmapEntity),
  Modified {
    vanilla: YmapEntity,
    diffs: Vec<YmapEntityStructDiffEnum>,
  },
}

impl YmapEntityDiff {
  pub fn extract_from(
    vanilla: &Ymap,
    modded: &Ymap,
  ) -> Vec<YmapEntityDiff> {
    check_entity_diff(vanilla, modded)
  }

  pub fn print_diffs(diffs: &Vec<YmapEntityDiff>) {
    log::info!("    Found {} entity differences", diffs.len());
    for diff in diffs {
      match diff {
        Self::Added(e) => {
          log::info!("      [Added] Entity: {} {}", e.guid, e.archetype_name);
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
          return_early!(if (diffs.is_empty()) return None);

          Some(YmapEntityDiff::Modified {
            vanilla: vanilla.clone(),
            diffs,
          })
        }
        (Some(_), None) => None,
        (None, Some(modded)) => Some(YmapEntityDiff::Added(modded.clone())),
        (None, None) => None,
      }
    })
    .collect::<Vec<_>>()
}
