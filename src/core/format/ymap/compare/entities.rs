//! Entity differences, the `#[diff(with)]` strategy of `Ymap::entities`.
//!
//! Entities sharing a GUID are paired one to one in entity order; when the counts differ, the
//! unpaired ones are reported as removed or added. Paired entities are compared as `YmapEntity`
//! declares, and their `parentIndex` by the entity it points at (see [`EntityParent`]).

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
    guid: u32,
  },
  /// Paired by GUID and differing beyond the tolerance, or pointing at another parent.
  Modified {
    guid: u32,
    before_index: usize,
    after_index: usize,
    changes: YmapEntityFieldDiff,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "typescript", specta(optional))]
    parent: Option<EntityParent>,
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
  parents.get(index).cloned().unwrap_or_else(|| {
    let entity = &entities[index];
    if entity.parent_index < 0 {
      EntityParent::None
    } else {
      EntityParent::Unresolved {
        in_parent: entity.flags & 8 != 0,
        index: entity.parent_index,
      }
    }
  })
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
  let pairs = guids
    .iter()
    .flat_map(|(_, vanilla_indices, modded_indices)| {
      vanilla_indices.iter().copied().zip(modded_indices.iter().copied())
    })
    .collect::<Vec<_>>();
  let anchors = anchors(&pairs, modded.len());
  let mut diffs = Vec::new();
  for (guid, vanilla_indices, modded_indices) in guids {
    for (&before_index, &after_index) in vanilla_indices.iter().zip(&modded_indices) {
      let changes = vanilla[before_index].diff(&modded[after_index], &ctx.tolerances);
      let parent = parent_of(&ctx.modded_parents, modded, after_index);
      let parent =
        (parent != parent_of(&ctx.vanilla_parents, vanilla, before_index)).then_some(parent);
      if changes.is_some() || parent.is_some() {
        diffs.push(YmapEntityDiff::Modified {
          guid,
          before_index,
          after_index,
          changes: changes.unwrap_or_default(),
          parent,
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
      parent: parent_of(&ctx.modded_parents, modded, index),
    }));
  }
  (!diffs.is_empty()).then_some(diffs)
}

/// Rebuilds `entities` from `diffs`, keeping unchanged entities in vanilla order.
///
/// `parentIndex` values are taken as they are: added entities keep the source's, and parent
/// changes are not applied. [`apply_linked`] also rebuilds the parents.
pub fn apply(
  entities: &mut Vec<YmapEntity>,
  diffs: &[YmapEntityDiff],
) -> Result<(), ApplyError> {
  let linked = entities.drain(..).map(|entity| (entity, EntityParent::None)).collect::<Vec<_>>();
  let (rebuilt, _) = apply_linked_items(linked, diffs)?;
  *entities = rebuilt;
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
  let guid = |index: usize| vanilla.get(index).map_or(0, |entity| entity.guid);
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
        guid: guid(index),
      }],
      // Entity diffs never replace, since entities pair by GUID; spelled out for completeness.
      ItemDiff::Replaced {
        index,
        item: (entity, parent),
      } => vec![
        YmapEntityDiff::Removed {
          index,
          guid: guid(index),
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
      } => vec![YmapEntityDiff::Modified {
        guid: guid(before_index),
        before_index,
        after_index,
        changes,
        parent,
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
