//! What an entity's `parentIndex` points at, independent of any entity array's order.
//!
//! `parentIndex` indexes the runtime entity order (regular entities, then MLO instances) of the
//! entity's own YMAP when the pair forms a valid local LOD hierarchy, and of its parent YMAP
//! otherwise, as CodeWalker resolves it. Diffs compare and record parents by the entity they name,
//! so an entity whose parent merely moved within its array is unchanged, and applying finds the
//! parent again in the final arrays.

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

/// The parent of every entity of a YMAP, in entity order.
pub type EntityParents = Vec<EntityParent>;

/// What comparing two YMAPs depends on: the matching tolerances and the parents of both YMAPs'
/// entities.
#[derive(Debug, Clone, Default)]
pub struct YmapDiffContext {
  pub tolerances: MatchTolerances,
  /// Parents of the vanilla YMAP's entities, in entity order.
  pub vanilla_parents: EntityParents,
  /// Parents of the modded YMAP's entities, in entity order.
  pub modded_parents: EntityParents,
}

impl YmapDiffContext {
  /// The context for comparing `vanilla` with `modded`, each given with the entities of its
  /// parent YMAP when available.
  pub fn new(
    tolerances: MatchTolerances,
    (vanilla, vanilla_parent): (&Ymap, Option<&[YmapEntity]>),
    (modded, modded_parent): (&Ymap, Option<&[YmapEntity]>),
  ) -> Self {
    Self {
      tolerances,
      vanilla_parents: entity_parents(vanilla, vanilla_parent),
      modded_parents: entity_parents(modded, modded_parent),
    }
  }
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
