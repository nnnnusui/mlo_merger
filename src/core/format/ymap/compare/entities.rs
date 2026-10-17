//! Entity differences, the `#[diff(with)]` strategy of `Ymap::entities`.
//!
//! Entities sharing a GUID are paired one to one in entity order; when the counts differ, the
//! unpaired ones are reported as removed or added. Paired entities are compared as `YmapEntity`
//! declares.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use struct_diff::{ApplyError, Diff};

use super::{
  field::anchors,
  item_diff::{ItemChange, MergedChange, MergedItems, apply_changes},
};
use crate::core::{
  config::matching::MatchTolerances,
  format::ymap::model::{YmapEntity, ymap_entity::YmapEntityFieldDiff},
};

/// How one entity of a modded YMAP differs from vanilla.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum YmapEntityDiff {
  /// Present only in the modded YMAP, right after vanilla entity `after`, or first when `None`.
  Added {
    after: Option<usize>,
    entity: YmapEntity,
  },
  /// Present only in vanilla, at `index` of its entities.
  Removed {
    index: usize,
    guid: u32,
  },
  /// Paired by GUID and differing beyond the tolerance.
  Modified {
    guid: u32,
    vanilla_index: usize,
    modded_index: usize,
    changes: YmapEntityFieldDiff,
  },
}

/// Indices into `entities` keyed by GUID, in entity order.
fn indices_by_guid(entities: &[YmapEntity]) -> HashMap<u32, Vec<usize>> {
  let mut indices = HashMap::<u32, Vec<usize>>::new();
  for (index, entity) in entities.iter().enumerate() {
    indices.entry(entity.guid).or_default().push(index);
  }
  indices
}

/// Differences of `modded` entities from `vanilla`; see the module docs for how they pair.
pub fn diff(
  vanilla: &[YmapEntity],
  modded: &[YmapEntity],
  tolerances: &MatchTolerances,
) -> Option<Vec<YmapEntityDiff>> {
  let (vanilla_by_guid, modded_by_guid) = (indices_by_guid(vanilla), indices_by_guid(modded));
  let mut seen = HashSet::new();
  let guids = vanilla
    .iter()
    .chain(modded)
    .map(|entity| entity.guid)
    .filter(|guid| seen.insert(*guid))
    .map(|guid| {
      let indices =
        |by_guid: &HashMap<u32, Vec<usize>>| by_guid.get(&guid).cloned().unwrap_or_default();
      (guid, indices(&vanilla_by_guid), indices(&modded_by_guid))
    })
    .collect::<Vec<_>>();
  let pairs = guids
    .iter()
    .flat_map(|(_, vanilla_indices, modded_indices)| {
      vanilla_indices.iter().copied().zip(modded_indices.iter().copied())
    })
    .collect::<Vec<_>>();
  let anchors = anchors(&pairs, modded.len());
  let mut diffs = Vec::new();
  for (guid, vanilla_indices, modded_indices) in guids {
    for (&vanilla_index, &modded_index) in vanilla_indices.iter().zip(&modded_indices) {
      if let Some(changes) = vanilla[vanilla_index].diff(&modded[modded_index], tolerances) {
        diffs.push(YmapEntityDiff::Modified {
          guid,
          vanilla_index,
          modded_index,
          changes,
        });
      }
    }
    let paired = vanilla_indices.len().min(modded_indices.len());
    diffs.extend(vanilla_indices[paired..].iter().map(|&index| YmapEntityDiff::Removed {
      index,
      guid,
    }));
    diffs.extend(modded_indices[paired..].iter().map(|&index| YmapEntityDiff::Added {
      after: anchors[index],
      entity: modded[index].clone(),
    }));
  }
  (!diffs.is_empty()).then_some(diffs)
}

/// Rebuilds `entities` from `diffs`, keeping unchanged entities in vanilla order.
pub fn apply(
  entities: &mut Vec<YmapEntity>,
  diffs: &[YmapEntityDiff],
) -> Result<(), ApplyError> {
  *entities = apply_changes(entities, diffs.iter().map(YmapEntityDiff::change), YmapEntity::apply)?;
  Ok(())
}

/// The entity diffs of several sources against `vanilla`, combined as
/// [`MergedItems`] describes; an addition equal to a removed vanilla entity is dropped too, so
/// deletion wins over re-adding.
pub(super) fn merge(
  vanilla: &[YmapEntity],
  sources: &[&[YmapEntityDiff]],
) -> Vec<YmapEntityDiff> {
  let changes = sources.iter().flat_map(|diffs| diffs.iter().map(YmapEntityDiff::change));
  let mut merged = MergedItems::collect("entities", changes);
  let removed = (0..vanilla.len())
    .filter(|index| merged.drops_vanilla(*index))
    .filter_map(|index| vanilla.get(index))
    .collect::<Vec<_>>();
  merged.retain_new(|entity| !removed.contains(&entity));
  let guid = |index: usize| vanilla.get(index).map_or(0, |entity| entity.guid);
  merged
    .into_changes(vanilla.len())
    .flat_map(|change| match change {
      MergedChange::Added {
        after,
        item,
      } => vec![YmapEntityDiff::Added {
        after,
        entity: item,
      }],
      MergedChange::Removed {
        index,
      } => vec![YmapEntityDiff::Removed {
        index,
        guid: guid(index),
      }],
      // Entity diffs never replace, since entities pair by GUID; spelled out for completeness.
      MergedChange::Replaced {
        index,
        item,
      } => vec![
        YmapEntityDiff::Removed {
          index,
          guid: guid(index),
        },
        YmapEntityDiff::Added {
          after: Some(index),
          entity: item,
        },
      ],
      MergedChange::Modified {
        vanilla_index,
        modded_index,
        changes,
      } => vec![YmapEntityDiff::Modified {
        guid: guid(vanilla_index),
        vanilla_index,
        modded_index,
        changes,
      }],
    })
    .collect()
}

impl YmapEntityDiff {
  /// The GUID of the entity this diff adds, removes or modifies.
  pub fn guid(&self) -> u32 {
    match self {
      Self::Added {
        entity,
        ..
      } => entity.guid,
      Self::Removed {
        guid,
        ..
      }
      | Self::Modified {
        guid,
        ..
      } => *guid,
    }
  }

  /// This diff as an [`ItemChange`].
  fn change(&self) -> ItemChange<'_, YmapEntity, YmapEntityFieldDiff> {
    match self {
      Self::Added {
        after,
        entity,
      } => ItemChange::Added {
        after: *after,
        item: entity,
      },
      Self::Removed {
        index,
        ..
      } => ItemChange::Removed {
        index: *index,
      },
      Self::Modified {
        vanilla_index,
        modded_index,
        changes,
        ..
      } => ItemChange::Modified {
        vanilla_index: *vanilla_index,
        modded_index: *modded_index,
        changes,
      },
    }
  }
}
