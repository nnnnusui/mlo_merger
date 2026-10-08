use super::*;

pub(super) fn runtime_entity_indices(entities: &[YmapEntity]) -> Vec<usize> {
  let mut indices = (0..entities.len()).collect::<Vec<_>>();
  indices.sort_by_key(|index| {
    reference_hash(&entities[*index].entity_type) == reference_hash("CMloInstanceDef")
  });
  indices
}

pub(super) fn lod_rank(name: &str) -> Option<usize> {
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
pub(in crate::core::merge) fn runtime_entities<'a>(
  entities: impl Iterator<Item = &'a YmapEntity>
) -> Vec<&'a YmapEntity> {
  let entities = entities.collect::<Vec<_>>();
  let mut indices = (0..entities.len()).collect::<Vec<_>>();
  indices.sort_by_key(|index| {
    reference_hash(&entities[*index].entity_type) == reference_hash("CMloInstanceDef")
  });
  indices.into_iter().map(|index| entities[index]).collect()
}

pub(super) fn is_local_parent(
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

/// Checks the same local LOD-level ordering used by CodeWalker EnsureEntities.
pub(in crate::core::merge) fn valid_local_pair(
  entity: &YmapEntity,
  parent: &YmapEntity,
) -> bool {
  match (lod_rank(&parent.lod_level), lod_rank(&entity.lod_level)) {
    (Some(parent), Some(child)) => parent > child && !(parent == 5 && child != 5),
    _ => false,
  }
}
