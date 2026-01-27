enum YmapEntityDiff {
  Added(YmapEntity),
  Removed(YmapEntity),
  Modified {
    vanilla: YmapEntity,
    modded: YmapEntity,
    diffs: Vec<YmapEntityFieldDiffEnum>,
  },
}

pub fn check_entity_diff(vanilla_ymap: &Ymap, mod_ymap: &Ymap) -> Vec<YmapStructDiffEnum> {
  let vanilla_entities_map = &vanilla_ymap.entity_map;
  let mod_entities_map = &mod_ymap.entity_map;
  let keys = vanilla_entities_map
    .keys()
    .chain(mod_entities_map.keys())
    .collect::<std::collections::HashSet<_>>();

  let entity_diffs = keys
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
            Some(YmapStructDiffEnum::entity_map(YmapEntityDiff::Modified {
              vanilla: vanilla.clone(),
              modded: modded.clone(),
              diffs,
            }))
          }
        }
        (Some(vanilla), None) => Some(YmapStructDiffEnum::entity_map(YmapEntityDiff::Removed(
          vanilla.clone(),
        ))),
        (None, Some(modded)) => Some(YmapStructDiffEnum::entity_map(YmapEntityDiff::Added(
          modded.clone(),
        ))),
        (None, None) => None,
      }
    })
    .collect::<Vec<_>>();

  entity_diffs
}
