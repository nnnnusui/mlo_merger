use crate::core::format::ymap::{diff::YmapEntityDiff, model::YmapEntity};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct Contribution {
  pub resource: String,
  pub path: PathBuf,
  pub change: Change,
  pub entity: Option<YmapEntity>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub(super) enum Change {
  Added,
  Modified,
  Removed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct IgnoredContribution {
  pub source: Contribution,
  pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct EntityDuplicate {
  pub guid: u32,
  pub vanilla: Option<YmapEntity>,
  pub applied: Contribution,
  pub ignored: Vec<IgnoredContribution>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub(super) struct DuplicateReport {
  pub format_version: u32,
  pub files: BTreeMap<String, Vec<EntityDuplicate>>,
}

#[derive(Default)]
pub(super) struct Collector {
  entries: BTreeMap<u32, Vec<Candidate>>,
}

struct Candidate {
  guid: u32,
  vanilla: Option<YmapEntity>,
  source: Contribution,
  signature: String,
}

impl Collector {
  /// Records candidates before deletion filtering or identical-diff deduplication.
  pub(super) fn add(
    &mut self,
    diff: &YmapEntityDiff,
    guid: u32,
    vanilla: Option<YmapEntity>,
    source: Contribution,
  ) {
    let key = match diff {
      YmapEntityDiff::Added(entity) | YmapEntityDiff::Removed(entity) => entity.guid,
      YmapEntityDiff::Modified {
        vanilla,
        ..
      } => vanilla.guid,
    };
    self.entries.entry(key).or_default().push(Candidate {
      guid,
      vanilla,
      source,
      signature: format!("{diff:?}"),
    });
  }

  /// Mirrors deletion-first, otherwise first-change-wins resolution in YmapDiff.
  pub(super) fn finish(self) -> Vec<EntityDuplicate> {
    self
      .entries
      .into_values()
      .filter_map(|candidates| {
        if candidates.len() < 2 {
          return None;
        }
        let index = candidates
          .iter()
          .position(|candidate| candidate.source.change == Change::Removed)
          .unwrap_or(0);
        let applied = &candidates[index];
        let ignored = candidates
          .iter()
          .enumerate()
          .filter(|(other, _)| *other != index)
          .map(|(_, candidate)| IgnoredContribution {
            source: candidate.source.clone(),
            reason: if candidate.signature == applied.signature {
              "identical_duplicate"
            } else if applied.source.change == Change::Removed {
              "deletion_wins"
            } else {
              "first_change_wins"
            }
            .into(),
          })
          .collect();
        Some(EntityDuplicate {
          guid: applied.guid,
          vanilla: applied.vanilla.clone(),
          applied: applied.source.clone(),
          ignored,
        })
      })
      .collect()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn duplicate_report_keeps_original_guids_and_deletion_decisions() {
    let xml: crate::core::format::ymap::xml::XmlYmap = quick_xml::de::from_str(include_str!(
      concat!(env!("CARGO_MANIFEST_DIR"), "/docs/sample/parent_refs/vanilla_parent.ymap.xml")
    ))
    .unwrap();
    let model: crate::core::format::ymap::model::Ymap = xml.into();
    let mut entity = model.entity_map.values().next().unwrap().clone();
    let guid = entity.guid;
    entity.guid = 1;
    let added = YmapEntityDiff::Added(entity.clone());
    let removed = YmapEntityDiff::Removed(entity);
    let source = |resource: &str, change| Contribution {
      resource: resource.into(),
      path: format!("{resource}/map.ymap").into(),
      change,
      entity: None,
    };
    let mut collector = Collector::default();
    collector.add(&added, guid, None, source("resource_a", Change::Added));
    collector.add(&removed, guid, None, source("resource_b", Change::Removed));
    collector.add(&removed, guid, None, source("resource_c", Change::Removed));
    let entries = collector.finish();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].guid, guid);
    assert_eq!(entries[0].applied.resource, "resource_b");
    assert_eq!(entries[0].ignored[0].reason, "deletion_wins");
    assert_eq!(entries[0].ignored[1].reason, "identical_duplicate");
    let json = serde_json::to_string(&entries).unwrap();
    assert!(json.contains("resource_c"));
  }
}
