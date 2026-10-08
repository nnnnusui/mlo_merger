use super::*;

/// A resolved parent identity, independent of an entity array's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::core::merge) struct ParentTarget {
  pub(in crate::core::merge) map: u32,
  pub(in crate::core::merge) guid: u32,
}

/// Resolve positional parent links before merging, then remap them to the final
/// entity order. Unresolved links are safe only while their original layout is unchanged.
#[derive(Default)]
pub(in crate::core::merge) struct ParentReferences {
  targets: Vec<ParentTarget>,
  handles: HashMap<ParentTarget, i32>,
  unresolved: HashMap<i32, UnresolvedReference>,
  unresolved_handles: HashMap<(Option<String>, u32, i32), i32>,
  ambiguous_guids: std::collections::HashSet<(u32, u32)>,
  entity_keys: HashMap<(u32, Vec<u32>, usize), u32>,
  original_guids: HashMap<u32, u32>,
}

struct UnresolvedReference {
  original_index: i32,
  original_layout: Option<Vec<u32>>,
}

impl ParentReferences {
  /// Registers ambiguous GUIDs before any source version is normalized.
  pub(in crate::core::merge) fn register_entities(
    &mut self,
    map: u32,
    entities: &[YmapEntity],
  ) {
    let mut seen = std::collections::HashSet::new();
    for entity in entities {
      if entity.guid == 0 || !seen.insert(entity.guid) {
        self.ambiguous_guids.insert((map, entity.guid));
      }
    }
  }

  /// Uses a unique GUID when possible, or a conservative geometric identity otherwise.
  #[cfg(test)]
  pub(in crate::core::merge) fn entity_id(
    &mut self,
    map: u32,
    entity: &YmapEntity,
  ) -> io::Result<u32> {
    self.entity_id_with_occurrence(map, entity_identity(self, map, entity), 0, entity.guid)
  }

  pub(in crate::core::merge) fn entity_ids(
    &mut self,
    map: u32,
    entities: &[YmapEntity],
  ) -> io::Result<Vec<u32>> {
    let mut occurrences = HashMap::<Vec<u32>, usize>::new();
    entities
      .iter()
      .map(|entity| {
        let identity = entity_identity(self, map, entity);
        let occurrence = occurrences.entry(identity.clone()).or_default();
        let current = *occurrence;
        *occurrence += 1;
        self.entity_id_with_occurrence(map, identity, current, entity.guid)
      })
      .collect()
  }

  pub(in crate::core::merge) fn runtime_entity_ids(
    &mut self,
    map: u32,
    entities: &[YmapEntity],
  ) -> io::Result<Vec<u32>> {
    let ids = self.entity_ids(map, entities)?;
    Ok(runtime_entity_indices(entities).into_iter().map(|index| ids[index]).collect())
  }

  fn entity_id_with_occurrence(
    &mut self,
    map: u32,
    identity: Vec<u32>,
    occurrence: usize,
    original_guid: u32,
  ) -> io::Result<u32> {
    let key = (map, identity, occurrence);
    if let Some(id) = self.entity_keys.get(&key) {
      return Ok(*id);
    }
    let id = u32::try_from(self.entity_keys.len())
      .ok()
      .and_then(|id| id.checked_add(1))
      .ok_or_else(|| invalid("too many internal entity identities"))?;
    self.entity_keys.insert(key, id);
    self.original_guids.insert(id, original_guid);
    Ok(id)
  }

  /// Restores original GUID values after all diffing and parent lookup have finished.
  pub(in crate::core::merge) fn original_guid(
    &self,
    id: u32,
  ) -> io::Result<u32> {
    self.original_guids.get(&id).copied().ok_or_else(|| invalid("unknown internal entity identity"))
  }
  /// Resolves an original array index and returns a merge-stable handle.
  pub(in crate::core::merge) fn capture(
    &mut self,
    map: u32,
    original: &[u32],
    index: i32,
  ) -> io::Result<i32> {
    if index < 0 {
      return Ok(index);
    }
    let guid = *original
      .get(index as usize)
      .ok_or_else(|| invalid("parentIndex is outside the original entity array"))?;
    if guid == 0 || original.iter().filter(|candidate| **candidate == guid).count() != 1 {
      return Err(invalid("referenced parent GUID is zero or ambiguous"));
    }
    let target = ParentTarget {
      map,
      guid,
    };
    if let Some(handle) = self.handles.get(&target) {
      return Ok(*handle);
    }
    let slot = i32::try_from(self.targets.len())
      .map_err(|_| invalid("too many resolved parent references"))?;
    let handle = slot
      .checked_add(2)
      .and_then(|value| value.checked_neg())
      .ok_or_else(|| invalid("parent reference handle overflow"))?;
    self.targets.push(target);
    self.handles.insert(target, handle);
    Ok(handle)
  }
  /// Returns the target represented by an internal handle.
  pub(in crate::core::merge) fn target(
    &self,
    handle: i32,
  ) -> Option<ParentTarget> {
    if self.unresolved.contains_key(&handle) {
      return None;
    }
    let index = handle.checked_neg()?.checked_sub(2)?;
    self.targets.get(usize::try_from(index).ok()?).copied()
  }

  /// Resolves a handle against a final array; removed parents are detached.
  pub(in crate::core::merge) fn restore(
    &self,
    handle: i32,
    final_guids: &[u32],
  ) -> io::Result<i32> {
    if handle == -1 {
      return Ok(-1);
    }
    if let Some(reference) = self.unresolved.get(&handle) {
      if reference.original_layout.as_deref() == Some(final_guids) {
        return Ok(reference.original_index);
      }
      return Err(invalid("cannot remap an unresolved parent after its entity array changed"));
    }
    let target =
      self.target(handle).ok_or_else(|| invalid("unresolved parent reference handle"))?;
    let mut matches = final_guids.iter().enumerate().filter(|(_, guid)| **guid == target.guid);
    let Some((index, _)) = matches.next() else {
      return Ok(-1);
    };
    if matches.next().is_some() {
      return Err(invalid("parent GUID is ambiguous in final entity array"));
    }
    i32::try_from(index).map_err(|_| invalid("final parent index exceeds i32"))
  }

  /// Records an unresolved reference without guessing its intended target.
  pub(in crate::core::merge) fn capture_unresolved(
    &mut self,
    namespace: Option<&str>,
    map: u32,
    original: Option<&[u32]>,
    index: i32,
  ) -> io::Result<i32> {
    let key = (namespace.map(str::to_string), map, index);
    if let Some(handle) = self.unresolved_handles.get(&key) {
      return Ok(*handle);
    }
    let slot =
      i32::try_from(self.targets.len()).map_err(|_| invalid("too many parent references"))?;
    let handle = slot
      .checked_add(2)
      .and_then(i32::checked_neg)
      .ok_or_else(|| invalid("parent handle overflow"))?;
    self.targets.push(ParentTarget {
      map,
      guid: 0,
    });
    self.unresolved.insert(
      handle,
      UnresolvedReference {
        original_index: index,
        original_layout: original.map(<[u32]>::to_vec),
      },
    );
    self.unresolved_handles.insert(key, handle);
    Ok(handle)
  }

  /// Returns the owning map hash, even for unresolved references.
  pub(in crate::core::merge) fn map_for(
    &self,
    handle: i32,
  ) -> Option<u32> {
    let index = handle.checked_neg()?.checked_sub(2)?;
    self.targets.get(usize::try_from(index).ok()?).map(|target| target.map)
  }

  /// Preserves an unresolved index only when its parent is unavailable in both inputs and outputs.
  pub(in crate::core::merge) fn unavailable_index(
    &self,
    handle: i32,
  ) -> Option<i32> {
    self
      .unresolved
      .get(&handle)
      .filter(|reference| reference.original_layout.is_none())
      .map(|reference| reference.original_index)
  }
}

fn entity_identity(
  references: &ParentReferences,
  map: u32,
  entity: &YmapEntity,
) -> Vec<u32> {
  let mut identity = vec![entity.guid];
  if entity.guid == 0 || references.ambiguous_guids.contains(&(map, entity.guid)) {
    identity.extend([
      reference_hash(&entity.entity_type),
      reference_hash(&entity.archetype_name),
      float_bits(entity.position.x),
      float_bits(entity.position.y),
      float_bits(entity.position.z),
      float_bits(entity.rotation.x),
      float_bits(entity.rotation.y),
      float_bits(entity.rotation.z),
      float_bits(entity.rotation.w),
      float_bits(entity.scale_x_y),
      float_bits(entity.scale_z),
    ]);
  }
  identity
}

fn float_bits(value: f32) -> u32 {
  if value == 0.0 { 0 } else { value.to_bits() }
}

#[cfg(test)]
mod tests {
  use super::super::tests::entity;
  use super::*;

  #[test]
  fn resolves_shifted_parent_indices_by_identity() {
    let mut original = (1..=202).collect::<Vec<u32>>();
    original[198] = 4241491920;
    let mut final_guids = original.clone();
    final_guids.swap(198, 200);
    let mut references = ParentReferences::default();
    let handle = references.capture(123, &original, 198).unwrap();
    assert_eq!(references.restore(handle, &final_guids).unwrap(), 200);
    assert_eq!(handle, references.capture(123, &final_guids, 200).unwrap());
    assert_ne!(handle, references.capture(456, &final_guids, 200).unwrap());
  }

  #[test]
  fn detaches_removed_parents_and_rejects_ambiguous_guids() {
    let mut references = ParentReferences::default();
    let handle = references.capture(1, &[10, 20], 1).unwrap();
    assert_eq!(references.restore(handle, &[10]).unwrap(), -1);
    assert!(references.capture(1, &[0], 0).is_err());
    assert!(references.capture(1, &[10, 10], 0).is_err());
    assert!(references.restore(handle, &[20, 20]).is_err());
    assert!(references.capture(1, &[10], 1).is_err());
  }

  #[test]
  fn ambiguous_guids_use_stable_geometry_and_restore_original_values() {
    for guid in [0, 42] {
      let mut first = entity(guid, "LODTYPES_DEPTH_LOD", -1);
      let mut second = first.clone();
      second.position.x += 10.0;
      let mut references = ParentReferences::default();
      references.register_entities(123, &[first.clone(), second.clone()]);
      let first_id = references.entity_id(123, &first).unwrap();
      let second_id = references.entity_id(123, &second).unwrap();
      assert_ne!(first_id, second_id);
      first.parent_index = 900;
      assert_eq!(references.entity_id(123, &first).unwrap(), first_id);
      assert_eq!(references.original_guid(first_id).unwrap(), guid);
      assert_ne!(references.entity_id(456, &first).unwrap(), first_id);
      let handle = references.capture(123, &[first_id, second_id], 0).unwrap();
      assert_eq!(references.restore(handle, &[second_id, first_id]).unwrap(), 1);
    }
  }

  #[test]
  fn unresolved_parent_layout_changes_fail_closed() {
    let mut references = ParentReferences::default();
    let handle = references.capture_unresolved(Some("mod"), 123, Some(&[10, 20]), 1).unwrap();
    assert_eq!(references.restore(handle, &[10, 20]).unwrap(), 1);
    assert!(references.restore(handle, &[20, 10]).is_err());
  }
}
