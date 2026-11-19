//! The changes several sources make to one vanilla entity or item of another array, which the
//! merge decides between, and the additions it leaves out because a source merged before makes
//! them already.

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
  /// Where its diff names the entity or item: the vanilla one's `base_index` for a removal, a
  /// modification or a replacement, the addition's `target_index` for an addition.
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
  /// An item put in place of the vanilla one.
  Replaced,
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
  /// The same resource adds the same entity or item in a newer generation.
  SameAdditionNewer,
  /// Its changes to the fields the applied modification leaves unchanged are applied with it,
  /// as for occlude models.
  Combined,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(super) struct IgnoredContribution {
  pub source: Contribution,
  pub reason: IgnoredReason,
}

/// What sources compete for.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(super) enum Target {
  /// An entity, by its GUID.
  Entity {
    guid: u32,
  },
  /// An item of another array, such as `lod_lights` or `distant_lod_lights.items`, by the
  /// vanilla item's index; an addition has none.
  Item {
    array: String,
    base_index: Option<usize>,
  },
}

/// The changes to one entity or item of which the merge applied one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(super) struct Competition {
  pub target: Target,
  pub applied: Contribution,
  pub ignored: Vec<IgnoredContribution>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(super) struct DuplicateReport {
  pub format_version: u32,
  pub files: BTreeMap<String, Vec<Competition>>,
}

#[derive(Default)]
pub(super) struct Collector {
  /// The stage of the vanilla YMAP the changes are merged into, as the archive names it.
  pub loaded_version: Option<String>,
  entries: BTreeMap<Target, Vec<Candidate>>,
  /// Additions left out for the same addition of a source merged before.
  thinned: Vec<Competition>,
}

struct Candidate {
  source: Contribution,
  signature: String,
}

impl Collector {
  /// Records a removal or modification of the vanilla entity `guid`, before deletion filtering
  /// or identical-diff deduplication. Additions are new entities, whatever their GUID: those
  /// left out are recorded with [`Self::add_ignored`], and the others all applied.
  pub(super) fn add(
    &mut self,
    diff: &YmapEntityDiff,
    guid: u32,
    source: Contribution,
  ) {
    if matches!(diff, YmapEntityDiff::Added { .. }) {
      return;
    }
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
    self
      .entries
      .entry(Target::Entity {
        guid,
      })
      .or_default()
      .push(Candidate {
        source,
        signature,
      });
  }

  /// Records a removal, modification or replacement of item `base_index` of the vanilla array
  /// `array`, `signature` telling the change apart from others without its indices.
  pub(super) fn add_item(
    &mut self,
    array: &str,
    base_index: usize,
    signature: String,
    source: Contribution,
  ) {
    let target = Target::Item {
      array: array.to_owned(),
      base_index: Some(base_index),
    };
    self.entries.entry(target).or_default().push(Candidate {
      source,
      signature,
    });
  }

  /// Records that the addition `left_out` of `target` was left out for `kept`, the same
  /// addition of a source merged before, for `reason`.
  pub(super) fn add_ignored(
    &mut self,
    target: Target,
    kept: Contribution,
    left_out: Contribution,
    reason: IgnoredReason,
  ) {
    self.thinned.push(Competition {
      target,
      applied: kept,
      ignored: vec![IgnoredContribution {
        source: left_out,
        reason,
      }],
    });
  }

  /// Mirrors the deletion-first, otherwise first-change-wins resolution of `compare::merge_diffs`,
  /// sources merged in order: a YMAP's own sources, then those carried from other generations,
  /// newest first. Modifications of one occlude model combine.
  pub(super) fn finish(self) -> Vec<Competition> {
    let mut entries = self
      .entries
      .into_iter()
      .filter_map(|(target, candidates)| {
        if candidates.len() < 2 {
          return None;
        }
        let index = candidates
          .iter()
          .position(|candidate| candidate.source.change == Change::Removed)
          .unwrap_or(0);
        let applied = &candidates[index];
        let combines = matches!(&target, Target::Item { array, .. } if array == "occlude_models");
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
            } else if combines
              && applied.source.change == Change::Modified
              && candidate.source.change == Change::Modified
            {
              IgnoredReason::Combined
            } else if newer(&applied.source, &candidate.source, self.loaded_version.as_deref()) {
              IgnoredReason::NewerGenerationWins
            } else {
              IgnoredReason::EarlierSourceWins
            },
          })
          .collect();
        Some(Competition {
          target,
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
/// loaded one, of the vanilla YMAP at stage `loaded_version`, older than a generation first
/// shipped after it, and a carried change from its origin.
pub(super) fn newer(
  applied: &Contribution,
  other: &Contribution,
  loaded_version: Option<&str>,
) -> bool {
  match (&applied.origin, &other.origin) {
    (None, Some(other)) => loaded_version.is_none_or(|loaded| other.as_str() < loaded),
    (Some(applied), Some(other)) => applied > other,
    _ => false,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn duplicate_report_keeps_original_guids_and_deletion_decisions() {
    let guid = 100;
    let modified = YmapEntityDiff::Modified {
      base_index: 0,
      target_index: 0,
      changes: crate::core::format::ymap::compare::YmapEntityFieldDiff {
        flags: Some(1),
        ..Default::default()
      },
      parent: None,
      reason: None,
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
    collector.add(&modified, guid, source("resource_a", Change::Modified));
    collector.add(&removed, guid, source("resource_b", Change::Removed));
    collector.add(&removed, guid, source("resource_c", Change::Removed));
    let entries = collector.finish();
    assert_eq!(entries.len(), 1);
    assert_eq!(
      entries[0].target,
      Target::Entity {
        guid
      }
    );
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

  #[test]
  fn items_of_other_arrays_compete_as_entities_do() {
    let source = |resource: &str, change| Contribution {
      resource: resource.into(),
      path: format!("{resource}/map.ymap").into(),
      change,
      index: Some(3),
      origin: None,
    };
    let mut collector = Collector::default();
    collector.add_item("lod_lights", 3, "Replaced a".into(), source("first", Change::Replaced));
    collector.add_item("lod_lights", 3, "Replaced b".into(), source("second", Change::Replaced));
    collector.add_item("occlude_models", 3, "Modified a".into(), source("first", Change::Modified));
    collector.add_item(
      "occlude_models",
      3,
      "Modified b".into(),
      source("second", Change::Modified),
    );
    collector.add_item("car_generators", 3, "Modified a".into(), source("first", Change::Modified));
    collector.add_item("car_generators", 3, "Removed".into(), source("second", Change::Removed));
    let entries = collector.finish();
    let reason = |array: &str| {
      let entry = entries
        .iter()
        .find(|entry| matches!(&entry.target, Target::Item { array: name, .. } if name == array))
        .unwrap();
      (entry.applied.resource.clone(), entry.ignored[0].reason)
    };
    assert_eq!(reason("lod_lights"), ("first".into(), IgnoredReason::EarlierSourceWins));
    assert_eq!(reason("occlude_models"), ("first".into(), IgnoredReason::Combined));
    assert_eq!(reason("car_generators"), ("second".into(), IgnoredReason::RemovalWins));
  }
}
