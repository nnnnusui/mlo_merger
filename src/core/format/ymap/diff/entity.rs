use crate::core::format::ymap::model::{Ymap, YmapEntity, ymap_entity::YmapEntityStructDiffEnum};
use std::collections::{HashMap, HashSet};
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
    let _span = tracing::info_span!("entities").entered();
    tracing::info!("Found {} differences", diffs.len());
    for diff in diffs {
      match diff {
        Self::Added(e) => {
          tracing::info!("[Added] Entity: {} {}", e.guid, e.archetype_name);
        }
        Self::Removed(e) => {
          tracing::info!("[Removed] Entity: {} {}", e.guid, e.archetype_name);
        }
        Self::Modified {
          vanilla: _,
          diffs,
        } => {
          tracing::info!("[Modified] Diffs: {:?}", diffs);
        }
      }
    }
  }
}

pub fn check_entity_diff(
  vanilla_ymap: &Ymap,
  mod_ymap: &Ymap,
) -> Vec<YmapEntityDiff> {
  let mut vanilla_by_guid = HashMap::<u32, Vec<&YmapEntity>>::new();
  for entity in &vanilla_ymap.entities {
    vanilla_by_guid.entry(entity.guid).or_default().push(entity);
  }
  let mut modded_by_guid = HashMap::<u32, Vec<&YmapEntity>>::new();
  for entity in &mod_ymap.entities {
    modded_by_guid.entry(entity.guid).or_default().push(entity);
  }

  let mut guids = Vec::new();
  let mut seen = HashSet::new();
  for entity in vanilla_ymap.entities.iter().chain(&mod_ymap.entities) {
    if seen.insert(entity.guid) {
      guids.push(entity.guid);
    }
  }

  let mut entity_diffs = Vec::new();
  for guid in guids {
    let vanilla = vanilla_by_guid.get(&guid).map(Vec::as_slice).unwrap_or_default();
    let modded = modded_by_guid.get(&guid).map(Vec::as_slice).unwrap_or_default();
    let paired_count = vanilla.len().min(modded.len());

    for (vanilla, modded) in vanilla.iter().zip(modded).take(paired_count) {
      let diffs = vanilla.diff(modded);
      if !diffs.is_empty() {
        entity_diffs.push(YmapEntityDiff::Modified {
          vanilla: (*vanilla).clone(),
          diffs,
        });
      }
    }
    entity_diffs.extend(
      vanilla.iter().skip(paired_count).map(|entity| YmapEntityDiff::Removed((*entity).clone())),
    );
    entity_diffs.extend(
      modded.iter().skip(paired_count).map(|entity| YmapEntityDiff::Added((*entity).clone())),
    );
  }
  entity_diffs
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
    let guid = vanilla.entities[0].guid;
    let mut deleted = vanilla.clone();
    deleted.entities.remove(0);
    let diffs = YmapEntityDiff::extract_from(&vanilla, &deleted);
    assert!(
      diffs
        .iter()
        .any(|diff| matches!(diff, YmapEntityDiff::Removed(entity) if entity.guid == guid))
    );
    let mut modified = vanilla.clone();
    modified.entities[0].position.x += 1.0;
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
      assert_eq!(merged.entities.len(), vanilla.entities.len() - 1);
      assert!(!merged.entities.iter().any(|entity| entity.guid == guid));
      let mut diff = YmapDiff::extract_from(&vanilla, &deleted);
      diff.entity_diffs.push(YmapEntityDiff::Added(vanilla.entities[0].clone()));
      if reverse {
        diff.entity_diffs.reverse();
      }
      assert!(!diff.apply_to(&vanilla, None).entities.iter().any(|entity| entity.guid == guid));
    }
  }

  #[test]
  fn entity_diff_applies_changes_to_each_duplicate_guid_occurrence() {
    let xml: XmlYmap = quick_xml::de::from_str(include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/docs/sample/parent_refs/vanilla_parent.ymap.xml"
    )))
    .unwrap();
    let mut vanilla: Ymap = xml.into();
    let first = vanilla.entities[0].clone();
    let mut second = first.clone();
    second.position.x += 10.0;
    vanilla.entities = vec![first, second];
    assert_eq!(vanilla.entities[0].guid, vanilla.entities[1].guid);

    let mut modded = vanilla.clone();
    modded.entities[0].position.y += 1.0;
    modded.entities[1].position.z += 2.0;
    let merged = YmapDiff::extract_from(&vanilla, &modded).apply_to(&vanilla, None);
    assert_eq!(merged.entities, modded.entities);

    let mut deleted = vanilla.clone();
    deleted.entities.remove(0);
    let merged = YmapDiff::extract_from(&vanilla, &deleted).apply_to(&vanilla, None);
    assert_eq!(merged.entities, deleted.entities);
  }
}
