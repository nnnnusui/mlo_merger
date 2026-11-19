//! The changes several sources make to one vanilla entity, which the merge decides between, and
//! the additions it leaves out because the same resource adds them in a newer generation.

use crate::core::format::ymap::compare::YmapEntityDiff;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

/// One source's change to an entity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(super) struct Contribution {
  pub resource: String,
  /// The source's file: its stream file or carried document, relative to its store.
  pub path: PathBuf,
  pub change: Change,
  /// Where its diff names the entity: the vanilla entity's `base_index` for a removal or a
  /// modification, the addition's `target_index` for an addition.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub index: Option<usize>,
  /// For a change carried from another generation, the stage first shipping that generation.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub origin: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(super) enum Change {
  Added,
  Modified,
  Removed,
}

/// Why a change was not applied.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum IgnoredReason {
  /// The applied change is the same.
  IdenticalDuplicate,
  /// A removal wins over every other change.
  RemovalWins,
  /// The applied change comes from a newer generation.
  NewerGenerationWins,
  /// The applied change comes from a source merged before.
  EarlierSourceWins,
  /// The same resource adds the same entity in a newer generation.
  SameAdditionNewer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(super) struct IgnoredContribution {
  pub source: Contribution,
  pub reason: IgnoredReason,
}

/// The changes to one entity of which the merge applied one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(super) struct EntityDuplicate {
  pub guid: u32,
  pub applied: Contribution,
  pub ignored: Vec<IgnoredContribution>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(super) struct DuplicateReport {
  pub format_version: u32,
  pub files: BTreeMap<String, Vec<EntityDuplicate>>,
}

#[derive(Default)]
pub(super) struct Collector {
  entries: BTreeMap<u32, Vec<Candidate>>,
  /// Additions left out for the same resource's in a newer generation.
  thinned: Vec<EntityDuplicate>,
}

struct Candidate {
  guid: u32,
  source: Contribution,
  signature: String,
}

impl Collector {
  /// Records candidates before deletion filtering or identical-diff deduplication.
  pub(super) fn add(
    &mut self,
    diff: &YmapEntityDiff,
    guid: u32,
    source: Contribution,
  ) {
    // Indices differ between sources, so identical changes are compared without them.
    let signature = match diff {
      YmapEntityDiff::Added {
        entity,
        ..
      } => format!("Added {entity:?}"),
      YmapEntityDiff::Removed {
        ..
      } => "Removed".into(),
      YmapEntityDiff::Modified {
        changes,
        ..
      } => format!("Modified {changes:?}"),
    };
    self.entries.entry(guid).or_default().push(Candidate {
      guid,
      source,
      signature,
    });
  }

  /// Records that the addition `left_out` of the entity `guid` was left out for `kept`, the same
  /// resource's addition in a newer generation.
  pub(super) fn add_thinned(
    &mut self,
    guid: u32,
    kept: Contribution,
    left_out: Contribution,
  ) {
    self.thinned.push(EntityDuplicate {
      guid,
      applied: kept,
      ignored: vec![IgnoredContribution {
        source: left_out,
        reason: IgnoredReason::SameAdditionNewer,
      }],
    });
  }

  /// Mirrors the deletion-first, otherwise first-change-wins resolution of `compare::merge_diffs`,
  /// sources merged in order: a YMAP's own sources, then those carried from other generations,
  /// newest first.
  pub(super) fn finish(self) -> Vec<EntityDuplicate> {
    let mut entries = self
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
              IgnoredReason::IdenticalDuplicate
            } else if applied.source.change == Change::Removed {
              IgnoredReason::RemovalWins
            } else if newer(&applied.source, &candidate.source) {
              IgnoredReason::NewerGenerationWins
            } else {
              IgnoredReason::EarlierSourceWins
            },
          })
          .collect();
        Some(EntityDuplicate {
          guid: applied.guid,
          applied: applied.source.clone(),
          ignored,
        })
      })
      .collect::<Vec<_>>();
    entries.extend(self.thinned);
    entries
  }
}

/// Whether `applied` comes from a newer generation than `other`: a YMAP's own source from the
/// loaded one, a carried change from its origin.
fn newer(
  applied: &Contribution,
  other: &Contribution,
) -> bool {
  match (&applied.origin, &other.origin) {
    (None, Some(_)) => true,
    (Some(applied), Some(other)) => applied > other,
    _ => false,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn duplicate_report_keeps_original_guids_and_deletion_decisions() {
    let model: crate::core::format::ymap::model::Ymap =
      crate::core::format::gamefile::test_support::sample_ymap("parent_refs/vanilla_parent.ymap");
    let mut entity = model.entities.first().unwrap().clone();
    let guid = entity.guid;
    entity.guid = 1;
    let added = YmapEntityDiff::Added {
      anchor_base_index: None,
      target_index: None,
      entity,
      parent: crate::core::format::ymap::compare::EntityParent::None,
    };
    let removed = YmapEntityDiff::Removed {
      base_index: 0,
      reason: None,
    };
    let source = |resource: &str, change| Contribution {
      resource: resource.into(),
      path: format!("{resource}/map.ymap").into(),
      change,
      index: Some(0),
      origin: None,
    };
    let mut collector = Collector::default();
    collector.add(&added, guid, source("resource_a", Change::Added));
    collector.add(&removed, guid, source("resource_b", Change::Removed));
    collector.add(&removed, guid, source("resource_c", Change::Removed));
    let entries = collector.finish();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].guid, guid);
    assert_eq!(entries[0].applied.resource, "resource_b");
    assert_eq!(entries[0].ignored[0].reason, IgnoredReason::RemovalWins);
    assert_eq!(entries[0].ignored[1].reason, IgnoredReason::IdenticalDuplicate);
    let json = serde_json::to_string(&entries).unwrap();
    assert!(json.contains("resource_c"));
  }

  #[test]
  fn a_newer_generations_modification_wins_over_an_older_ones() {
    let modified = |flags| YmapEntityDiff::Modified {
      base_index: 0,
      target_index: 0,
      changes: crate::core::format::ymap::compare::YmapEntityFieldDiff {
        flags: Some(flags),
        ..Default::default()
      },
      parent: None,
      reason: None,
    };
    let source = |resource: &str, origin: Option<&str>| Contribution {
      resource: resource.into(),
      path: format!("{resource}/map.ymap").into(),
      change: Change::Modified,
      index: Some(0),
      origin: origin.map(str::to_owned),
    };
    let mut collector = Collector::default();
    collector.add(&modified(1), 7, source("own", None));
    collector.add(&modified(2), 7, source("heist", Some("0012-mpheist")));
    collector.add(&modified(3), 7, source("base", Some("0000-base")));
    let entries = collector.finish();
    assert_eq!(entries[0].applied.resource, "own");
    assert!(
      entries[0].ignored.iter().all(|ignored| ignored.reason == IgnoredReason::NewerGenerationWins)
    );
  }
}
