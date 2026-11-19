//! What an entity's `parentIndex` points at, independent of any entity array's order.
//!
//! `parentIndex` indexes the runtime entity order (regular entities, then MLO instances) of the
//! entity's own YMAP when the pair forms a valid local LOD hierarchy, and of its parent YMAP
//! otherwise, as CodeWalker resolves it. Diffs compare and record parents by the entity they name,
//! so an entity whose parent merely moved within its array is unchanged, and applying finds the
//! parent again in the final arrays.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::core::{
  config::matching::MatchTolerances,
  format::ymap::{
    model::{Ymap, YmapEntity},
    reference_hash,
    relation::{is_local_parent, runtime_entities},
  },
};

/// The entity an entity's `parentIndex` points at.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum EntityParent {
  /// `parentIndex` is negative.
  None,
  /// The entity of the same YMAP with this GUID.
  Own {
    guid: u32,
  },
  /// The entity with this GUID of the parent YMAP, whose name has the reference hash `map`.
  Parent {
    map: u32,
    guid: u32,
  },
  /// An index naming no entity with a unique, non-zero GUID, or into a parent YMAP that is
  /// unavailable; kept as is, so it holds only while the array it indexes is unchanged.
  Unresolved {
    in_parent: bool,
    index: i32,
  },
}

impl EntityParent {
  /// The parent `entity`'s `parentIndex` names, taken as a raw index rather than resolved.
  pub fn raw(entity: &YmapEntity) -> Self {
    if entity.parent_index < 0 {
      Self::None
    } else {
      Self::Unresolved {
        in_parent: entity.flags & 8 != 0,
        index: entity.parent_index,
      }
    }
  }
}

/// The parent of every entity of a YMAP, in entity order.
pub type EntityParents = Vec<EntityParent>;

/// GUIDs of modded entities that a tool rounded through a 32-bit float, by the vanilla GUID they
/// were rounded from.
pub type GuidAliases = HashMap<u32, u32>;

/// What comparing two YMAPs depends on: the matching tolerances, the parents of both YMAPs'
/// entities, and the GUIDs of the modded YMAP's entities that were rounded.
#[derive(Debug, Clone, Default)]
pub struct YmapDiffContext {
  pub tolerances: MatchTolerances,
  /// Parents of the vanilla YMAP's entities, in entity order.
  pub vanilla_parents: EntityParents,
  /// Parents of the modded YMAP's entities, in entity order.
  pub modded_parents: EntityParents,
  /// Records entities that move among the kept ones as removed and added again, so applying
  /// rebuilds their order (see [`super::entities`]). Merging would take them for deletions, so
  /// only diffs replayed onto the YMAP they were taken from record moves.
  pub record_moves: bool,
  /// The modded YMAP's entity GUIDs rounded from vanilla ones (see [`rounded_guid_aliases`]);
  /// those entities pair with vanilla's by the vanilla GUID, which they keep.
  pub guid_aliases: GuidAliases,
}

impl YmapDiffContext {
  /// The context for comparing `vanilla` with `modded`, each given with the entities of its
  /// parent YMAP when available.
  pub fn new(
    tolerances: MatchTolerances,
    (vanilla, vanilla_parent): (&Ymap, Option<&[YmapEntity]>),
    (modded, modded_parent): (&Ymap, Option<&[YmapEntity]>),
  ) -> Self {
    let tolerance = tolerances.ymap;
    let guid_aliases = rounded_guid_aliases(&vanilla.entities, &modded.entities, tolerance);
    let parent_aliases = match (vanilla_parent, modded_parent) {
      (Some(vanilla), Some(modded)) => rounded_guid_aliases(vanilla, modded, tolerance),
      _ => GuidAliases::new(),
    };
    let alias = |aliases: &GuidAliases, guid: u32| aliases.get(&guid).copied().unwrap_or(guid);
    let modded_parents = entity_parents(modded, modded_parent)
      .into_iter()
      .map(|parent| match parent {
        EntityParent::Own {
          guid,
        } => EntityParent::Own {
          guid: alias(&guid_aliases, guid),
        },
        EntityParent::Parent {
          map,
          guid,
        } => EntityParent::Parent {
          map,
          guid: alias(&parent_aliases, guid),
        },
        parent => parent,
      })
      .collect();
    Self {
      tolerances,
      vanilla_parents: entity_parents(vanilla, vanilla_parent),
      modded_parents,
      record_moves: false,
      guid_aliases,
    }
  }
}

/// The GUIDs of `modded` entities that a tool rounded through a 32-bit float, by the `vanilla`
/// GUID each was rounded from.
///
/// A float holds integers exactly only up to 2^24, so such a tool writes most GUIDs with their
/// low bits changed. A modded entity's GUID is taken for rounded from a vanilla entity's when
/// neither GUID is on the other side, the modded one is unique and as a float equals the vanilla
/// one, and the entity is [placed alike](super::added::placed_alike), within `tolerance`.
pub fn rounded_guid_aliases(
  vanilla: &[YmapEntity],
  modded: &[YmapEntity],
  tolerance: f32,
) -> GuidAliases {
  let guids =
    |entities: &[YmapEntity]| entities.iter().map(|entity| entity.guid).collect::<HashSet<_>>();
  let (vanilla_guids, modded_guids) = (guids(vanilla), guids(modded));
  let mut candidates = HashMap::<u32, Vec<&YmapEntity>>::new();
  for entity in
    vanilla.iter().filter(|entity| entity.guid != 0 && !modded_guids.contains(&entity.guid))
  {
    candidates.entry((entity.guid as f32).to_bits()).or_default().push(entity);
  }
  let mut counts = HashMap::<u32, usize>::new();
  for entity in modded {
    *counts.entry(entity.guid).or_default() += 1;
  }
  let mut aliases = GuidAliases::new();
  for entity in modded {
    let guid = entity.guid;
    if guid == 0 || vanilla_guids.contains(&guid) || counts[&guid] > 1 {
      continue;
    }
    let Some(candidates) = candidates.get_mut(&(guid as f32).to_bits()) else {
      continue;
    };
    if let Some(position) =
      candidates.iter().position(|vanilla| super::added::placed_alike(entity, vanilla, tolerance))
    {
      aliases.insert(guid, candidates.remove(position).guid);
    }
  }
  aliases
}

/// The parent of each entity of `ymap` as its raw `parentIndex` (see [`EntityParent::raw`]), for
/// diffs that are replayed onto the YMAP they were taken from rather than merged: every changed
/// index is recorded, and [`super::entities::apply`] sets it again.
pub fn raw_entity_parents(ymap: &Ymap) -> EntityParents {
  ymap.entities.iter().map(EntityParent::raw).collect()
}

/// The parent of each entity of `ymap`, given the entities of its parent YMAP when available.
pub fn entity_parents(
  ymap: &Ymap,
  parent_entities: Option<&[YmapEntity]>,
) -> EntityParents {
  let own = runtime_entities(ymap.entities.iter());
  let parent = parent_entities.map(|entities| runtime_entities(entities.iter()));
  let map = reference_hash(&ymap.parent);
  ymap
    .entities
    .iter()
    .map(|entity| {
      let Ok(index) = usize::try_from(entity.parent_index) else {
        return EntityParent::None;
      };
      let unresolved = |in_parent| EntityParent::Unresolved {
        in_parent,
        index: entity.parent_index,
      };
      if is_local_parent(entity, &own) {
        return unique_guid(&own, index).map_or(unresolved(false), |guid| EntityParent::Own {
          guid,
        });
      }
      match &parent {
        Some(parent) if map != 0 => {
          unique_guid(parent, index).map_or(unresolved(true), |guid| EntityParent::Parent {
            map,
            guid,
          })
        }
        _ => unresolved(true),
      }
    })
    .collect()
}

/// The GUID of `entities[index]` when it is non-zero and no other entity has it.
fn unique_guid(
  entities: &[&YmapEntity],
  index: usize,
) -> Option<u32> {
  let guid = entities.get(index)?.guid;
  (guid != 0 && entities.iter().filter(|entity| entity.guid == guid).count() == 1).then_some(guid)
}
