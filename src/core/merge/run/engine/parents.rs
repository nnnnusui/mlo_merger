//! Linking the planned entities to their parents in the final entity orders.

use super::*;

/// A parent reference a planned entity keeps: the YMAP holding the entity, by name hash, its
/// index there, and its parent.
type ResolvedLink = (u32, usize, ParentTarget);

impl<'a> MergeRun<'a> {
  /// Plans the vanilla YMAPs no source changes whose parent's entities moved, for their parent
  /// indices to be repaired; returns the name hashes of the planned YMAPs whose entities moved.
  pub(super) fn relink_vanilla_children(
    &mut self
  ) -> Result<HashSet<u32>, Box<dyn std::error::Error>> {
    let mut changed_layouts = HashSet::new();
    for (hash, output) in &self.planned {
      // Children of an added YMAP point into the YMAP its resource ships, not its baseline.
      let name = output.name.to_ascii_lowercase();
      let original = if self.merge.added.contains(&name) && !self.merge.replacing.contains(&name) {
        Some(Rc::clone(&output.original))
      } else {
        self.sources.get(None, *hash)?
      };
      if let Some(vanilla) = original {
        let original_layout = self.references.runtime_entity_ids(*hash, &vanilla.entities)?;
        if output.guids() != original_layout {
          changed_layouts.insert(*hash);
        }
      }
    }
    let mut vanilla_children = Vec::new();
    // Only children of a map whose entity order changed need relinking, and planned maps are
    // relinked already, so the other vanilla maps are not read.
    let candidates = if changed_layouts.is_empty() { &[][..] } else { self.latest_files };
    for path in candidates {
      let name = path.file_name().ok_or("Vanilla YMAP filename missing")?;
      let name = name.to_str().ok_or("non-UTF8 vanilla filename")?;
      let hash = reference_hash(name.trim_end_matches(".ymap"));
      if self.planned.contains_key(&hash) {
        continue;
      }
      let Some(original) = self.sources.get(None, hash)? else {
        continue;
      };
      if changed_layouts.contains(&reference_hash(&original.model.parent)) {
        vanilla_children.push((hash, name.to_string()));
      }
    }
    for (hash, name) in vanilla_children {
      if self.planned.contains_key(&hash) {
        continue;
      }
      let original = self.sources.get(None, hash)?.ok_or("vanilla child unavailable")?;
      let (model, entities) =
        self.sources.normalize(&original, None, hash, &mut self.references)?;
      self.planned.insert(
        hash,
        PlannedMap {
          name,
          model,
          clone_entities: Some(entities),
          original,
          copy_target: None,
          rebuild: false,
        },
      );
    }
    Ok(changed_layouts)
  }

  /// Restores every planned entity's parent index from the handle it was linked with, against
  /// the final entity order of the YMAP it points into, detaching those whose parent was removed
  /// or lies in a YMAP the merged one does not declare. Returns the children counted for each
  /// parent, and the parent references kept.
  #[allow(clippy::type_complexity)]
  pub(super) fn restore_parent_indices(
    &mut self
  ) -> Result<(HashMap<(u32, u32), u32>, Vec<ResolvedLink>), Box<dyn std::error::Error>> {
    let mut layouts =
      self.planned.iter().map(|(hash, output)| (*hash, output.guids())).collect::<HashMap<_, _>>();
    let mut child_counts = HashMap::<(u32, u32), u32>::new();
    let mut resolved_links = Vec::new();
    for (hash, output) in &mut self.planned {
      let parent_hash = reference_hash(&output.model.parent);
      let map_name = output.name.clone();
      let declared_parent = output.model.parent.clone();
      let entities = output.entities_mut();
      for (child_index, entity) in entities.into_iter().enumerate() {
        let handle = entity.parent_index;
        if handle == -1 {
          continue;
        }
        let owner = self.references.map_for(handle).ok_or("missing normalized parent handle")?;
        if owner != *hash && owner != parent_hash {
          tracing::warn!(
            "Entity {} in {} references parent YMAP {owner:08X}, but the merged map declares {parent_hash:08X} ({}); detaching the external parent link",
            entity.guid,
            map_name,
            declared_parent
          );
          entity.parent_index = -1;
          detach(entity);
          continue;
        }
        if let std::collections::hash_map::Entry::Vacant(entry) = layouts.entry(owner)
          && let Some(vanilla) = self.sources.original(owner)?
        {
          self.references.register_entities(owner, &vanilla.entities);
          let layout = self.references.runtime_entity_ids(owner, &vanilla.entities)?;
          entry.insert(layout);
        }
        let index = if let Some(layout) = layouts.get(&owner) {
          self
            .references
            .restore(handle, layout)
            .map_err(|error| format!("entity {} parent map {owner:08X}: {error}", entity.guid))?
        } else {
          self
            .references
            .unavailable_index(handle)
            .ok_or_else(|| format!("resolved parent map {owner:08X} has no final output"))?
        };
        if let Some(target) = self.references.target(handle) {
          if index >= 0 {
            *child_counts.entry((target.map, target.guid)).or_default() += 1;
            if owner == *hash {
              entity.flags &= !8;
            } else {
              entity.flags |= 8;
            }
            resolved_links.push((*hash, child_index, target));
          } else {
            tracing::warn!(
              "    Parent {} in map {:08X} removed; detaching entity {}",
              target.guid,
              target.map,
              entity.guid
            );
            detach(entity);
          }
        }
        entity.parent_index = index;
      }
    }
    Ok((child_counts, resolved_links))
  }

  /// Checks that every parent reference within one YMAP forms a valid local LOD hierarchy in its
  /// final entities.
  pub(super) fn check_local_hierarchies(
    &self,
    resolved_links: Vec<ResolvedLink>,
  ) -> Result<(), Box<dyn std::error::Error>> {
    // Each map is assembled once, however many of its entities link within it.
    let mut final_maps = HashMap::<u32, Ymap>::new();
    for (hash, index, target) in resolved_links {
      if target.map != hash {
        continue;
      }
      let map = &self.planned[&hash];
      let child = map.entity_at(index).ok_or("final child entity missing")?;
      let final_map = final_maps.entry(hash).or_insert_with(|| {
        let mut final_map = map.model.clone();
        if let Some(entities) = &map.clone_entities {
          final_map.entities.clone_from(entities);
        }
        final_map
      });
      if entity_parent_ref(child, final_map, final_map) != EntityParentRef::SelfMap(target.guid) {
        return Err(
          format!(
            "invalid final local LOD hierarchy for entity {} and parent {}",
            child.guid, target.guid
          )
          .into(),
        );
      }
    }
    Ok(())
  }
}

/// Detaches `entity` from its parent: it no longer points into its parent YMAP, and a
/// high-detail entity becomes an orphan.
fn detach(entity: &mut YmapEntity) {
  entity.flags &= !8;
  if reference_hash(&entity.lod_level) == reference_hash("LODTYPES_DEPTH_HD") {
    entity.lod_level = "LODTYPES_DEPTH_ORPHANHD".into();
  }
}
