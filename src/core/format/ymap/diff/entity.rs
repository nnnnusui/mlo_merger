use crate::{
  core::format::ymap::model::{Ymap, YmapEntity, ymap_entity::YmapEntityStructDiffEnum},
  return_early,
};
use structdiff::StructDiff;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum YmapEntityDiff {
  Added(YmapEntity),
  /// Removes this entity; deletion takes precedence over additions or modifications.
  Removed(YmapEntity),
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
        Self::Removed(e) => {
          log::info!("      [Removed] Entity: {} {}", e.guid, e.archetype_name);
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
        (Some(vanilla), None) => Some(YmapEntityDiff::Removed(vanilla.clone())),
        (None, Some(modded)) => Some(YmapEntityDiff::Added(modded.clone())),
        (None, None) => None,
      }
    })
    .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::format::ymap::{diff::YmapDiff, xml::XmlYmap};

  #[test]
  fn entity_deletion_wins_over_modification_and_readdition_in_every_order() {
    let xml: XmlYmap = quick_xml::de::from_str(include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/docs/sample/parent_refs/vanilla_parent.ymap.xml"
    )))
    .unwrap();
    let vanilla: Ymap = xml.into();
    let guid = *vanilla.entity_map.keys().next().unwrap();
    let mut deleted = vanilla.clone();
    deleted.entity_map.shift_remove(&guid);
    let diffs = YmapEntityDiff::extract_from(&vanilla, &deleted);
    assert!(
      diffs
        .iter()
        .any(|diff| matches!(diff, YmapEntityDiff::Removed(entity) if entity.guid == guid))
    );
    let mut modified = vanilla.clone();
    modified.entity_map.get_mut(&guid).unwrap().position.x += 1.0;
    for reverse in [false, true] {
      let delete = YmapDiff::extract_from(&vanilla, &deleted);
      let modify = YmapDiff::extract_from(&vanilla, &modified);
      let merged_diff = if reverse { modify.merge(delete) } else { delete.merge(modify) };
      assert!(
        merged_diff
          .entity_diffs
          .iter()
          .all(|diff| matches!(diff, YmapEntityDiff::Removed(entity) if entity.guid == guid))
      );
      let merged_diff = merged_diff.merge(YmapDiff::extract_from(&vanilla, &modified));
      assert!(
        merged_diff
          .entity_diffs
          .iter()
          .all(|diff| matches!(diff, YmapEntityDiff::Removed(entity) if entity.guid == guid))
      );
      let merged = merged_diff.apply_to(&vanilla, None);
      assert!(!merged.entity_map.contains_key(&guid));
      assert_eq!(merged.entity_map.len(), vanilla.entity_map.len() - 1);
      let mut diff = YmapDiff::extract_from(&vanilla, &deleted);
      diff.entity_diffs.push(YmapEntityDiff::Added(vanilla.entity_map[&guid].clone()));
      if reverse {
        diff.entity_diffs.reverse();
      }
      assert!(!diff.apply_to(&vanilla, None).entity_map.contains_key(&guid));
    }
  }
}
