//! Entity differences, the `#[diff(with)]` strategy of `Ymap::entities`.
//!
//! Entities sharing a GUID are paired one to one in entity order; when the counts differ, the
//! unpaired ones are reported as removed or added. Paired entities are compared as `YmapEntity`
//! declares.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use struct_diff::{ApplyError, Diff};

use super::item_diff::{ItemChange, apply_changes};
use crate::core::{
  config::matching::MatchTolerances,
  format::ymap::model::{YmapEntity, ymap_entity::YmapEntityFieldDiff},
};

/// How one entity of a modded YMAP differs from vanilla.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum YmapEntityDiff {
  /// Present only in the modded YMAP, at `index` of its entities.
  Added {
    index: usize,
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
  let mut diffs = Vec::new();
  let mut seen = HashSet::new();
  let guids = vanilla.iter().chain(modded).map(|entity| entity.guid);
  for guid in guids.filter(|guid| seen.insert(*guid)) {
    let vanilla_indices = vanilla_by_guid.get(&guid).map_or(&[][..], Vec::as_slice);
    let modded_indices = modded_by_guid.get(&guid).map_or(&[][..], Vec::as_slice);
    for (&vanilla_index, &modded_index) in vanilla_indices.iter().zip(modded_indices) {
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
      index,
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
  let changes = diffs.iter().map(|diff| match diff {
    YmapEntityDiff::Added {
      index,
      entity,
    } => ItemChange::Added {
      index: *index,
      item: entity,
    },
    YmapEntityDiff::Removed {
      index,
      ..
    } => ItemChange::Removed {
      index: *index,
    },
    YmapEntityDiff::Modified {
      vanilla_index,
      modded_index,
      changes,
      ..
    } => ItemChange::Modified {
      vanilla_index: *vanilla_index,
      modded_index: *modded_index,
      changes,
    },
  });
  *entities = apply_changes(entities, changes, YmapEntity::apply)?;
  Ok(())
}
