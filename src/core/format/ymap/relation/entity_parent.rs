use super::super::{
  model::{Ymap, YmapEntity},
  reference_hash,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntityParentRef {
  None,
  SelfMap(u32),
  Parent(u32),
}

pub(crate) fn runtime_entity_indices(entities: &[YmapEntity]) -> Vec<usize> {
  let mut indices = (0..entities.len()).collect::<Vec<_>>();
  indices.sort_by_key(|index| {
    reference_hash(&entities[*index].entity_type) == reference_hash("CMloInstanceDef")
  });
  indices
}

pub(crate) fn lod_rank(name: &str) -> Option<usize> {
  [
    "LODTYPES_DEPTH_HD",
    "LODTYPES_DEPTH_LOD",
    "LODTYPES_DEPTH_SLOD1",
    "LODTYPES_DEPTH_SLOD2",
    "LODTYPES_DEPTH_SLOD3",
    "LODTYPES_DEPTH_ORPHANHD",
    "LODTYPES_DEPTH_SLOD4",
  ]
  .iter()
  .position(|candidate| reference_hash(candidate) == reference_hash(name))
}

/// Returns CodeWalker's runtime array order: regular entities followed by MLO instances.
pub(crate) fn runtime_entities<'a>(
  entities: impl Iterator<Item = &'a YmapEntity>
) -> Vec<&'a YmapEntity> {
  let entities = entities.collect::<Vec<_>>();
  let mut indices = (0..entities.len()).collect::<Vec<_>>();
  indices.sort_by_key(|index| {
    reference_hash(&entities[*index].entity_type) == reference_hash("CMloInstanceDef")
  });
  indices.into_iter().map(|index| entities[index]).collect()
}

pub(crate) fn is_local_parent(
  entity: &YmapEntity,
  entities: &[&YmapEntity],
) -> bool {
  if entity.flags & 8 != 0 {
    return false;
  }
  let Some(parent) =
    usize::try_from(entity.parent_index).ok().and_then(|index| entities.get(index))
  else {
    return false;
  };
  valid_local_pair(entity, parent)
}

pub(crate) fn entity_parent_ref(
  entity: &YmapEntity,
  self_ymap: &Ymap,
  parent_ymap: &Ymap,
) -> EntityParentRef {
  let Ok(parent_index) = usize::try_from(entity.parent_index) else {
    return EntityParentRef::None;
  };

  if entity.flags & 8 != 0 {
    runtime_entities(parent_ymap.entities.iter())
      .get(parent_index)
      .map(|parent| EntityParentRef::Parent(parent.guid))
      .unwrap_or(EntityParentRef::None)
  } else {
    let entities = runtime_entities(self_ymap.entities.iter());
    let Some(parent) = entities.get(parent_index) else {
      return EntityParentRef::None;
    };
    if valid_local_pair(entity, parent) {
      EntityParentRef::SelfMap(parent.guid)
    } else {
      EntityParentRef::None
    }
  }
}

/// Checks the same local LOD-level ordering used by CodeWalker EnsureEntities.
fn valid_local_pair(
  entity: &YmapEntity,
  parent: &YmapEntity,
) -> bool {
  match (lod_rank(&parent.lod_level), lod_rank(&entity.lod_level)) {
    (Some(parent), Some(child)) => parent > child && !(parent == 5 && child != 5),
    _ => false,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn sample_map() -> Ymap {
    crate::core::format::gamefile::test_support::sample_ymap("parent_refs/vanilla_parent.ymap")
  }

  #[test]
  fn resolves_parent_indices_to_self_or_external_parent_guid() {
    let mut self_ymap = sample_map();
    let mut local_parent = self_ymap.entities[0].clone();
    local_parent.guid = 100;
    local_parent.entity_type = "CEntityDef".into();
    local_parent.lod_level = "LODTYPES_DEPTH_SLOD1".into();
    let mut child = self_ymap.entities[1].clone();
    child.parent_index = 0;
    child.flags &= !8;
    child.lod_level = "LODTYPES_DEPTH_HD".into();
    self_ymap.entities = vec![local_parent, child.clone()];
    assert_eq!(entity_parent_ref(&child, &self_ymap, &sample_map()), EntityParentRef::SelfMap(100));
    self_ymap.entities[0].lod_level = "LODTYPES_DEPTH_ORPHANHD".into();
    assert_eq!(entity_parent_ref(&child, &self_ymap, &sample_map()), EntityParentRef::None);

    child.flags |= 8;
    let mut parent_ymap = sample_map();
    parent_ymap.entities[0].guid = 200;
    assert_eq!(entity_parent_ref(&child, &self_ymap, &parent_ymap), EntityParentRef::Parent(200));
    child.parent_index = -1;
    assert_eq!(entity_parent_ref(&child, &self_ymap, &parent_ymap), EntityParentRef::None);
  }
}
