use std::{
  collections::HashMap,
  io,
  path::{Path, PathBuf},
  rc::Rc,
};

use super::ymap_metadata_diff::reference_hash;
use crate::core::format::ymap::{
  model::{Ymap, YmapEntity},
  xml::XmlYmap,
};

/// A resolved parent identity, independent of an entity array's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ParentTarget {
  pub(super) map: u32,
  pub(super) guid: u32,
}

/// Interns resolved parents as temporary indices below -1 during merging.
#[derive(Default)]
pub(super) struct ParentReferences {
  targets: Vec<ParentTarget>,
  handles: HashMap<ParentTarget, i32>,
  unresolved: HashMap<i32, UnresolvedReference>,
  unresolved_handles: HashMap<(Option<String>, u32, i32), i32>,
  ambiguous_guids: std::collections::HashSet<(u32, u32)>,
  entity_keys: HashMap<(u32, Vec<u32>), u32>,
  original_guids: HashMap<u32, u32>,
}

struct UnresolvedReference {
  original_index: i32,
  original_layout: Option<Vec<u32>>,
}

impl ParentReferences {
  /// Registers ambiguous GUIDs before any source version is normalized.
  pub(super) fn register_entities(
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
  pub(super) fn entity_id(
    &mut self,
    map: u32,
    entity: &YmapEntity,
  ) -> io::Result<u32> {
    let mut identity = vec![entity.guid];
    if entity.guid == 0 || self.ambiguous_guids.contains(&(map, entity.guid)) {
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
    let key = (map, identity);
    if let Some(id) = self.entity_keys.get(&key) {
      return Ok(*id);
    }
    let id = u32::try_from(self.entity_keys.len())
      .ok()
      .and_then(|id| id.checked_add(1))
      .ok_or_else(|| invalid("too many internal entity identities"))?;
    self.entity_keys.insert(key, id);
    self.original_guids.insert(id, entity.guid);
    Ok(id)
  }

  /// Restores original GUID values after all diffing and parent lookup have finished.
  pub(super) fn original_guid(
    &self,
    id: u32,
  ) -> io::Result<u32> {
    self.original_guids.get(&id).copied().ok_or_else(|| invalid("unknown internal entity identity"))
  }
  /// Resolves an original array index and returns a merge-stable handle.
  pub(super) fn capture(
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
  pub(super) fn target(
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
  pub(super) fn restore(
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
  pub(super) fn capture_unresolved(
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
  pub(super) fn map_for(
    &self,
    handle: i32,
  ) -> Option<u32> {
    let index = handle.checked_neg()?.checked_sub(2)?;
    self.targets.get(usize::try_from(index).ok()?).map(|target| target.map)
  }

  /// Preserves an unresolved index only when its parent is unavailable in both inputs and outputs.
  pub(super) fn unavailable_index(
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

/// Original XML and entity order, retained separately from the GUID-keyed merge model.
pub(super) struct OriginalMap {
  pub(super) model: Ymap,
  pub(super) entities: Vec<YmapEntity>,
  pub(super) xml: String,
}

impl OriginalMap {
  /// Loads without losing duplicate GUID entries from the original entity array.
  pub(super) fn load(path: &Path) -> io::Result<Self> {
    let xml = std::fs::read_to_string(path)?;
    let mut parsed: XmlYmap = quick_xml::de::from_str(&xml)
      .map_err(|error| invalid(&format!("{}: {error}", path.display())))?;
    if let Some(error) =
      parsed.instanced_data.error.as_ref().filter(|error| !error.trim().is_empty())
    {
      return Err(invalid(&format!("{}: instancedData error: {error}", path.display())));
    }
    let entities = std::mem::take(&mut parsed.entities.items)
      .into_iter()
      .map(YmapEntity::from)
      .collect::<Vec<_>>();
    let mut model: Ymap = parsed.into();
    model.entity_map = entities.iter().map(|entity| (entity.guid, entity.clone())).collect();
    Ok(Self {
      model,
      entities,
      xml,
    })
  }
}

/// Lazy original-map lookup: same-mod parent first, vanilla parent otherwise.
#[derive(Default)]
pub(super) struct SourceMaps {
  paths: HashMap<(Option<String>, u32), PathBuf>,
  loaded: HashMap<(Option<String>, u32), Rc<OriginalMap>>,
}

impl SourceMaps {
  /// Registers a map by its stream filename, not its potentially unresolved XML name.
  pub(super) fn register(
    &mut self,
    namespace: Option<&str>,
    name: &str,
    path: PathBuf,
  ) -> io::Result<()> {
    let name = name.strip_suffix(".xml").unwrap_or(name);
    let name = name.strip_suffix(".ymap").unwrap_or(name);
    let key = (namespace.map(str::to_string), reference_hash(name));
    if self.paths.insert(key, path).is_some() {
      return Err(invalid("duplicate original map name within one resource"));
    }
    Ok(())
  }

  /// Loads a requested map and caches only maps actually involved in resolution.
  pub(super) fn get(
    &mut self,
    namespace: Option<&str>,
    hash: u32,
  ) -> io::Result<Option<Rc<OriginalMap>>> {
    let preferred = (namespace.map(str::to_string), hash);
    let key = if self.paths.contains_key(&preferred) { preferred } else { (None, hash) };
    if let Some(map) = self.loaded.get(&key) {
      return Ok(Some(Rc::clone(map)));
    }
    let Some(path) = self.paths.get(&key) else {
      return Ok(None);
    };
    let map = Rc::new(OriginalMap::load(path)?);
    self.loaded.insert(key, Rc::clone(&map));
    Ok(Some(map))
  }

  /// Normalizes original parent indices into merge-stable handles.
  pub(super) fn normalize(
    &mut self,
    original: &OriginalMap,
    namespace: Option<&str>,
    map_hash: u32,
    references: &mut ParentReferences,
  ) -> io::Result<(Ymap, Vec<YmapEntity>)> {
    references.register_entities(map_hash, &original.entities);
    let mut entities = original.entities.clone();
    let own_ids = original
      .entities
      .iter()
      .map(|entity| references.entity_id(map_hash, entity))
      .collect::<io::Result<Vec<_>>>()?;
    let original_order = runtime_entities(original.entities.iter());
    let own_layout = original_order
      .iter()
      .map(|entity| references.entity_id(map_hash, entity))
      .collect::<io::Result<Vec<_>>>()?;
    let external = reference_hash(&original.model.parent);
    for (entity_index, entity) in entities.iter_mut().enumerate() {
      entity.guid = own_ids[entity_index];
      let index = entity.parent_index;
      if index < 0 {
        entity.parent_index = -1;
        continue;
      }
      let local = is_local_parent(entity, &original_order);
      let owner = if local { map_hash } else { external };
      let parent = if local { None } else { self.get(namespace, owner)? };
      let layout = if local {
        Some(own_layout.clone())
      } else {
        parent
          .as_ref()
          .map(|parent| {
            references.register_entities(owner, &parent.entities);
            runtime_entities(parent.entities.iter())
              .into_iter()
              .map(|entity| references.entity_id(owner, entity))
              .collect::<io::Result<Vec<_>>>()
          })
          .transpose()?
      };
      entity.parent_index = match layout
        .as_deref()
        .map(|layout| references.capture(owner, layout, index))
        .transpose()
      {
        Ok(Some(handle)) => handle,
        Ok(None) => {
          log::warn!(
            "    Parent map {owner:08X} unavailable for entity {} index {index}; preserving only if parent remains unavailable",
            entity.guid
          );
          references.capture_unresolved(namespace, owner, None, index)?
        }
        Err(error) => {
          log::warn!(
            "    Unresolved parent for entity {}: {error}; changes to its parent layout will be rejected",
            entity.guid
          );
          references.capture_unresolved(namespace, owner, layout.as_deref(), index)?
        }
      };
    }
    let mut model = original.model.clone();
    model.entity_map = entities.iter().map(|entity| (entity.guid, entity.clone())).collect();
    Ok((model, entities))
  }
}

fn lod_rank(name: &str) -> Option<usize> {
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
pub(super) fn runtime_entities<'a>(
  entities: impl Iterator<Item = &'a YmapEntity>
) -> Vec<&'a YmapEntity> {
  let mut ordered = entities.collect::<Vec<_>>();
  ordered
    .sort_by_key(|entity| reference_hash(&entity.entity_type) == reference_hash("CMloInstanceDef"));
  ordered
}

fn is_local_parent(
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
pub(super) fn valid_local_pair(
  entity: &YmapEntity,
  parent: &YmapEntity,
) -> bool {
  match (lod_rank(&parent.lod_level), lod_rank(&entity.lod_level)) {
    (Some(parent), Some(child)) => parent > child && !(parent == 5 && child != 5),
    _ => false,
  }
}

/// Patches only parent-related XML fields in a promoted clone, preserving all other payloads.
pub(super) fn patch_clone(
  original: &OriginalMap,
  entities: &[YmapEntity],
) -> io::Result<String> {
  use quick_xml::{
    Reader, Writer,
    events::{BytesStart, BytesText, Event},
  };
  let mut reader = Reader::from_str(&original.xml);
  let mut writer = Writer::new(Vec::new());
  let mut stack = Vec::<String>::new();
  let mut count = 0usize;
  let mut current = None;
  loop {
    let event = reader.read_event().map_err(|error| invalid(&error.to_string()))?;
    let empty = matches!(&event, Event::Empty(_));
    match event {
      Event::Start(start) | Event::Empty(start) => {
        let name = String::from_utf8_lossy(start.name().as_ref()).into_owned();
        if name == "Item" && stack.last().is_some_and(|parent| parent == "entities") {
          current = Some(count);
          count += 1;
        }
        let direct = stack.last().is_some_and(|parent| parent == "Item")
          && stack.get(stack.len().saturating_sub(2)).is_some_and(|parent| parent == "entities");
        let replacement = if direct {
          let entity = entities
            .get(current.ok_or_else(|| invalid("clone entity context missing"))?)
            .ok_or_else(|| invalid("clone entity count differs"))?;
          match name.as_str() {
            "parentIndex" => Some(entity.parent_index.to_string()),
            "flags" => Some(entity.flags.to_string()),
            "numChildren" => Some(entity.num_children.to_string()),
            _ => None,
          }
        } else {
          None
        };
        let element = if let Some(value) = replacement {
          let mut element = BytesStart::new(name.clone());
          element.push_attribute(("value", value.as_str()));
          element.into_owned()
        } else {
          start.into_owned()
        };
        writer
          .write_event(if empty { Event::Empty(element) } else { Event::Start(element) })
          .map_err(|error| invalid(&error.to_string()))?;
        if !empty {
          stack.push(name);
        }
      }
      Event::Text(text)
        if stack.last().is_some_and(|name| name == "lodLevel")
          && stack.get(stack.len().saturating_sub(2)).is_some_and(|name| name == "Item")
          && stack.get(stack.len().saturating_sub(3)).is_some_and(|name| name == "entities") =>
      {
        let entity = &entities[current.ok_or_else(|| invalid("clone entity context missing"))?];
        writer
          .write_event(Event::Text(BytesText::new(&entity.lod_level)))
          .map_err(|error| invalid(&error.to_string()))?;
        drop(text);
      }
      Event::End(end) => {
        writer
          .write_event(Event::End(end.into_owned()))
          .map_err(|error| invalid(&error.to_string()))?;
        stack.pop();
      }
      Event::Eof => break,
      event => {
        writer.write_event(event.into_owned()).map_err(|error| invalid(&error.to_string()))?
      }
    }
  }
  if count != entities.len() {
    return Err(invalid("clone entity count differs from original XML"));
  }
  String::from_utf8(writer.into_inner()).map_err(|_| invalid("clone XML is not UTF-8"))
}

fn invalid(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

fn float_bits(value: f32) -> u32 {
  if value == 0.0 { 0 } else { value.to_bits() }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn entity(
    guid: u32,
    lod_level: &str,
    parent_index: i32,
  ) -> YmapEntity {
    YmapEntity {
      entity_type: "CEntityDef".into(),
      archetype_name: "road".into(),
      flags: 0,
      guid,
      position: crate::core::common::position::Position::default(),
      rotation: crate::core::common::rotation::Rotation {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        w: 1.0,
      },
      scale_x_y: 1.0,
      scale_z: 1.0,
      parent_index,
      lod_dist: 100.0,
      child_lod_dist: 100.0,
      lod_level: lod_level.into(),
      num_children: 0,
      priority_level: "PRI_REQUIRED".into(),
      ambient_occlusion_multiplier: 255,
      artificial_ambient_occlusion: 255,
      tint_value: 0,
    }
  }

  fn original_map(entities: Vec<YmapEntity>) -> OriginalMap {
    let mut parsed: XmlYmap = quick_xml::de::from_str(r#"<CMapData>
      <name>map</name><parent>external</parent><flags value="0"/><contentFlags value="0"/>
      <streamingExtentsMin x="0" y="0" z="0"/><streamingExtentsMax x="0" y="0" z="0"/>
      <entitiesExtentsMin x="0" y="0" z="0"/><entitiesExtentsMax x="0" y="0" z="0"/>
      <block><version value="0"/><flags value="0"/><name>map</name><exportedBy>test</exportedBy><owner/><time/></block>
    </CMapData>"#).unwrap();
    parsed.entities.items = entities.iter().cloned().map(Into::into).collect();
    let xml = quick_xml::se::to_string(&parsed).unwrap();
    OriginalMap {
      model: parsed.into(),
      entities,
      xml,
    }
  }

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
  fn clone_patch_preserves_original_order_duplicate_guids_and_payloads() {
    let mut second = entity(0, "LODTYPES_DEPTH_HD", 198);
    second.position.x = 10.0;
    let mut original = original_map(vec![entity(0, "LODTYPES_DEPTH_HD", 198), second]);
    original.xml = original.xml.replacen(
      "<extensions/>",
      "<extensions><Item type=\"test\"><payload>keep</payload></Item></extensions>",
      1,
    );
    let mut entities = original.entities.clone();
    entities[0].parent_index = 202;
    entities[0].flags = 8;
    entities[1].parent_index = -1;
    entities[1].lod_level = "LODTYPES_DEPTH_ORPHANHD".into();
    entities[1].num_children = 3;
    let patched = patch_clone(&original, &entities).unwrap();
    assert!(patched.contains("<payload>keep</payload>"));
    let parsed: XmlYmap = quick_xml::de::from_str(&patched).unwrap();
    let actual = parsed.entities.items.into_iter().map(YmapEntity::from).collect::<Vec<_>>();
    assert_eq!(actual, entities);
    assert_eq!(
      actual.iter().filter(|entity| entity.guid == 0).count(),
      original.entities.iter().filter(|entity| entity.guid == 0).count()
    );
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

  #[test]
  fn normalization_distinguishes_local_lod_and_explicit_external_parents() {
    let mut external = entity(30, "LODTYPES_DEPTH_HD", 0);
    external.flags = 8;
    let original = original_map(vec![
      entity(10, "LODTYPES_DEPTH_LOD", -1),
      entity(20, "LODTYPES_DEPTH_HD", 0),
      external,
    ]);
    let mut references = ParentReferences::default();
    let (_, normalized) =
      SourceMaps::default().normalize(&original, None, 123, &mut references).unwrap();
    let local = references.target(normalized[1].parent_index).unwrap();
    assert_eq!(local.map, 123);
    assert_eq!(local.guid, normalized[0].guid);
    assert_eq!(references.map_for(normalized[2].parent_index), Some(reference_hash("external")));
    assert_eq!(references.unavailable_index(normalized[2].parent_index), Some(0));
    assert!(!valid_local_pair(&original.entities[1], &entity(99, "LODTYPES_DEPTH_ORPHANHD", -1)));
  }

  #[test]
  fn indistinguishable_duplicates_remain_ambiguous_without_losing_clone_entities() {
    let first = entity(42, "LODTYPES_DEPTH_LOD", -1);
    let original = original_map(vec![first.clone(), first]);
    let mut references = ParentReferences::default();
    let (model, entities) =
      SourceMaps::default().normalize(&original, None, 123, &mut references).unwrap();
    assert_eq!(entities.len(), 2);
    assert_eq!(model.entity_map.len(), 1);
    assert!(references.capture(123, &[entities[0].guid, entities[1].guid], 0).is_err());
  }

  #[test]
  fn mixed_mlo_arrays_resolve_runtime_indices_without_reordering_clone_xml() {
    let mut mlo = entity(99, "LODTYPES_DEPTH_HD", -1);
    mlo.entity_type = "CMloInstanceDef".into();
    let original = original_map(vec![
      mlo,
      entity(10, "LODTYPES_DEPTH_LOD", -1),
      entity(20, "LODTYPES_DEPTH_HD", 0),
    ]);
    let mut references = ParentReferences::default();
    let (_, entities) =
      SourceMaps::default().normalize(&original, None, 123, &mut references).unwrap();
    let target = references.target(entities[2].parent_index).unwrap();
    assert_eq!(target.guid, entities[1].guid);
    let layout =
      runtime_entities(entities.iter()).into_iter().map(|entity| entity.guid).collect::<Vec<_>>();
    assert_eq!(references.restore(entities[2].parent_index, &layout).unwrap(), 0);
    assert_eq!(entities[0].entity_type, "CMloInstanceDef");
  }
}
