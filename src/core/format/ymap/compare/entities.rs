//! Entity differences, the `#[diff(with)]` strategy of `Ymap::entities`.
//!
//! Entities sharing a GUID are paired one to one in entity order; when the counts differ, the
//! unpaired ones are reported as removed or added. Paired entities are compared as `YmapEntity`
//! declares, and their `parentIndex` by the entity it points at (see [`EntityParent`]).
//!
//! Applying keeps paired entities in vanilla order, so a reordering is not recorded unless
//! [`YmapDiffContext::record_moves`] asks: then the fewest paired entities whose removal leaves
//! the rest in order are reported as removed and added again, at their modded place.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use struct_diff::{ApplyError, Diff};

use struct_diff::vec::{ItemChange, ItemDiff, anchors, apply_changes};

use super::{
  entity_parent::{EntityParent, YmapDiffContext},
  merged_items::MergedItems,
};
use crate::core::format::ymap::model::{YmapEntity, ymap_entity::YmapEntityFieldDiff};

/// How one entity of a modded YMAP differs from vanilla.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum YmapEntityDiff {
  /// Present only in the modded YMAP, right after vanilla entity `after`, or first when `None`,
  /// with the parent it points at.
  Added {
    after: Option<usize>,
    entity: YmapEntity,
    parent: EntityParent,
  },
  /// Present only in vanilla, at `index` of its entities.
  Removed {
    index: usize,
    /// Why the entity is removed, when the removal was not taken from the modded YMAP itself;
    /// see [`ItemDiff::Removed`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", specta(optional))]
    reason: Option<String>,
  },
  /// Paired by GUID and differing beyond the tolerance, or pointing at another parent; the
  /// entity is vanilla's at `before_index` and the modded YMAP's at `after_index`.
  Modified {
    before_index: usize,
    after_index: usize,
    changes: YmapEntityFieldDiff,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", specta(optional))]
    parent: Option<EntityParent>,
    /// Why the entity is modified, when the changes were not taken from the modded YMAP itself;
    /// see [`ItemDiff::Modified`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", specta(optional))]
    reason: Option<String>,
  },
}

/// Changes of one entity: its fields and, when it points at another parent, that parent.
type EntityChanges = (YmapEntityFieldDiff, Option<EntityParent>);

/// An entity with the parent it points at.
type Linked = (YmapEntity, EntityParent);

/// Indices into `entities` keyed by GUID, in entity order.
fn indices_by_guid(entities: &[YmapEntity]) -> HashMap<u32, Vec<usize>> {
  let mut indices = HashMap::<u32, Vec<usize>>::new();
  for (index, entity) in entities.iter().enumerate() {
    indices.entry(entity.guid).or_default().push(index);
  }
  indices
}

/// The parent of `entities[index]` from `parents`, or its raw index when `parents` does not
/// cover it.
fn parent_of(
  parents: &[EntityParent],
  entities: &[YmapEntity],
  index: usize,
) -> EntityParent {
  parents.get(index).cloned().unwrap_or_else(|| EntityParent::raw(&entities[index]))
}

/// Differences of `modded` entities from `vanilla`; see the module docs for how they pair.
pub fn diff(
  vanilla: &[YmapEntity],
  modded: &[YmapEntity],
  ctx: &YmapDiffContext,
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
  let mut pairs = guids
    .iter()
    .flat_map(|(_, vanilla_indices, modded_indices)| {
      vanilla_indices.iter().copied().zip(modded_indices.iter().copied())
    })
    .collect::<Vec<_>>();
  let moved = if ctx.record_moves { moved_pairs(&mut pairs) } else { HashSet::new() };
  let anchors = anchors(&pairs, modded.len());
  let mut diffs = Vec::new();
  // By modded index, since additions after one vanilla entity are applied in diff order.
  let mut additions = Vec::new();
  let added = |index: usize| YmapEntityDiff::Added {
    after: anchors[index],
    entity: modded[index].clone(),
    parent: parent_of(&ctx.modded_parents, modded, index),
  };
  for (_, vanilla_indices, modded_indices) in guids {
    for (&before_index, &after_index) in vanilla_indices.iter().zip(&modded_indices) {
      if moved.contains(&before_index) {
        diffs.push(YmapEntityDiff::Removed {
          index: before_index,
          reason: None,
        });
        additions.push((after_index, added(after_index)));
        continue;
      }
      let changes = vanilla[before_index].diff(&modded[after_index], &ctx.tolerances);
      let parent = parent_of(&ctx.modded_parents, modded, after_index);
      let parent =
        (parent != parent_of(&ctx.vanilla_parents, vanilla, before_index)).then_some(parent);
      if changes.is_some() || parent.is_some() {
        diffs.push(YmapEntityDiff::Modified {
          before_index,
          after_index,
          changes: changes.unwrap_or_default(),
          parent,
          reason: None,
        });
      }
    }
    let paired = vanilla_indices.len().min(modded_indices.len());
    diffs.extend(vanilla_indices[paired..].iter().map(|&index| YmapEntityDiff::Removed {
      index,
      reason: None,
    }));
    additions.extend(modded_indices[paired..].iter().map(|&index| (index, added(index))));
  }
  additions.sort_by_key(|(index, _)| *index);
  diffs.extend(additions.into_iter().map(|(_, diff)| diff));
  (!diffs.is_empty()).then_some(diffs)
}

/// Removes from `pairs` the fewest whose removal leaves the rest in the same order on both sides,
/// returning their vanilla indices.
fn moved_pairs(pairs: &mut Vec<(usize, usize)>) -> HashSet<usize> {
  pairs.sort_unstable();
  // The longest run of pairs increasing in modded index: `tails[length - 1]` is the pair ending
  // the best run of that length found so far, and `previous` links each pair to the one before
  // it in its run.
  let mut tails = Vec::<usize>::new();
  let mut previous = vec![None; pairs.len()];
  for (position, &(_, after_index)) in pairs.iter().enumerate() {
    let length = tails.partition_point(|&tail| pairs[tail].1 < after_index);
    previous[position] = length.checked_sub(1).map(|length| tails[length]);
    if length == tails.len() {
      tails.push(position);
    } else {
      tails[length] = position;
    }
  }
  let mut kept = vec![false; pairs.len()];
  let mut position = tails.last().copied();
  while let Some(current) = position {
    kept[current] = true;
    position = previous[current];
  }
  let mut kept = kept.into_iter();
  let mut moved = HashSet::new();
  pairs.retain(|&(before_index, _)| {
    let keep = kept.next().unwrap_or(true);
    if !keep {
      moved.insert(before_index);
    }
    keep
  });
  moved
}

/// Rebuilds `entities` from `diffs`, keeping unchanged entities in vanilla order.
///
/// Parent changes to no parent or to a raw index are applied to `parentIndex`. Those naming an
/// entity by GUID need the final entity arrays, so the index is kept as it is, and added entities
/// keep the source's; [`apply_linked`] returns those parents for merging to resolve.
pub fn apply(
  entities: &mut Vec<YmapEntity>,
  diffs: &[YmapEntityDiff],
) -> Result<(), ApplyError> {
  let linked = entities
    .drain(..)
    .map(|entity| {
      let parent = EntityParent::raw(&entity);
      (entity, parent)
    })
    .collect::<Vec<_>>();
  let (rebuilt, parents) = apply_linked_items(linked, diffs)?;
  *entities = rebuilt
    .into_iter()
    .zip(parents)
    .map(|(mut entity, parent)| {
      match parent {
        EntityParent::None if entity.parent_index >= 0 => entity.parent_index = -1,
        EntityParent::Unresolved {
          index,
          ..
        } => entity.parent_index = index,
        _ => {}
      }
      entity
    })
    .collect();
  Ok(())
}

/// [`apply`] for `entities` together with their `parents`, returning the rebuilt entities and
/// the parent each points at, in order.
pub fn apply_linked(
  entities: &[YmapEntity],
  parents: &[EntityParent],
  diffs: &[YmapEntityDiff],
) -> Result<(Vec<YmapEntity>, Vec<EntityParent>), ApplyError> {
  let linked =
    (0..entities.len()).map(|index| (entities[index].clone(), parent_of(parents, entities, index)));
  apply_linked_items(linked.collect(), diffs)
}

fn apply_linked_items(
  linked: Vec<Linked>,
  diffs: &[YmapEntityDiff],
) -> Result<(Vec<YmapEntity>, Vec<EntityParent>), ApplyError> {
  let changes = diffs.iter().map(YmapEntityDiff::change).collect::<Vec<_>>();
  let rebuilt = apply_changes(
    &linked,
    changes.iter().map(Change::as_item),
    |(entity, parent), (changes, new_parent)| {
      entity.apply(changes)?;
      if let Some(new_parent) = new_parent {
        parent.clone_from(new_parent);
      }
      Ok(())
    },
  )?;
  Ok(rebuilt.into_iter().unzip())
}

/// The entity diffs of several sources against `vanilla`, combined as
/// [`MergedItems`] describes; an addition equal to a removed vanilla entity is dropped too, so
/// deletion wins over re-adding.
pub(super) fn merge(
  vanilla: &[YmapEntity],
  sources: &[&[YmapEntityDiff]],
) -> Vec<YmapEntityDiff> {
  let changes =
    sources.iter().flat_map(|diffs| diffs.iter().map(YmapEntityDiff::change)).collect::<Vec<_>>();
  let mut merged = MergedItems::collect("entities", changes.iter().map(Change::as_item));
  let removed = (0..vanilla.len())
    .filter(|index| merged.drops_vanilla(*index))
    .filter_map(|index| vanilla.get(index))
    .collect::<Vec<_>>();
  merged.retain_new(|(entity, _)| !removed.contains(&entity));
  merged
    .into_diffs(vanilla.len())
    .into_iter()
    .flat_map(|change| match change {
      ItemDiff::Added {
        after,
        item: (entity, parent),
      } => vec![YmapEntityDiff::Added {
        after,
        entity,
        parent,
      }],
      ItemDiff::Removed {
        index,
        ..
      } => vec![YmapEntityDiff::Removed {
        index,
        reason: None,
      }],
      // Entity diffs never replace, since entities pair by GUID; spelled out for completeness.
      ItemDiff::Replaced {
        index,
        item: (entity, parent),
      } => vec![
        YmapEntityDiff::Removed {
          index,
          reason: None,
        },
        YmapEntityDiff::Added {
          after: Some(index),
          entity,
          parent,
        },
      ],
      ItemDiff::Modified {
        before_index,
        after_index,
        changes: (changes, parent),
        ..
      } => vec![YmapEntityDiff::Modified {
        before_index,
        after_index,
        changes,
        parent,
        reason: None,
      }],
    })
    .collect()
}

impl YmapEntityDiff {
  /// The GUID of the entity this diff adds, or of the entity of `vanilla`, the entities the
  /// diff was taken against, that it removes or modifies; 0 when the index is out of range.
  pub fn guid(
    &self,
    vanilla: &[YmapEntity],
  ) -> u32 {
    let index = match self {
      Self::Added {
        entity,
        ..
      } => return entity.guid,
      Self::Removed {
        index,
        ..
      } => *index,
      Self::Modified {
        before_index,
        ..
      } => *before_index,
    };
    vanilla.get(index).map_or(0, |entity| entity.guid)
  }

  /// This diff as an owned [`Change`].
  fn change(&self) -> Change {
    match self {
      Self::Added {
        after,
        entity,
        parent,
      } => Change::Added {
        after: *after,
        item: (entity.clone(), parent.clone()),
      },
      Self::Removed {
        index,
        ..
      } => Change::Removed {
        index: *index,
      },
      Self::Modified {
        before_index,
        after_index,
        changes,
        parent,
        ..
      } => Change::Modified {
        before_index: *before_index,
        after_index: *after_index,
        changes: (changes.clone(), parent.clone()),
      },
    }
  }
}

/// An entity change owning its linked entity and changes, lent out as an [`ItemChange`].
enum Change {
  Added {
    after: Option<usize>,
    item: Linked,
  },
  Removed {
    index: usize,
  },
  Modified {
    before_index: usize,
    after_index: usize,
    changes: EntityChanges,
  },
}

impl Change {
  fn as_item(&self) -> ItemChange<'_, Linked, EntityChanges> {
    match self {
      Self::Added {
        after,
        item,
      } => ItemChange::Added {
        after: *after,
        item,
      },
      Self::Removed {
        index,
      } => ItemChange::Removed {
        index: *index,
      },
      Self::Modified {
        before_index,
        after_index,
        changes,
      } => ItemChange::Modified {
        before_index: *before_index,
        after_index: *after_index,
        changes,
      },
    }
  }
}
