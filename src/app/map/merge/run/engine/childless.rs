//! Removing the parent entities the merge takes the last children of.
//!
//! A removed parent's own parent loses a child in turn, so removals are repeated until no
//! parent loses its last child.

use super::*;

impl<'a> MergeRun<'a> {
  /// Removes every entity left without children by the merge, whose vanilla version has
  /// children, then its parents left without children in turn, relinking the children of the
  /// YMAPs whose entities moved each time. Returns the name hashes of the planned YMAPs whose
  /// entities moved.
  pub(super) fn remove_childless_parents(
    &mut self
  ) -> Result<HashSet<u32>, Box<dyn std::error::Error>> {
    loop {
      let changed_layouts = self.relink_vanilla_children()?;
      let child_counts = self.count_children()?;
      let emptied = self
        .counted_children(&changed_layouts, &child_counts)?
        .into_iter()
        .filter(|(_, counted)| counted.emptied())
        .map(|(key, _)| key)
        .collect::<Vec<_>>();
      if self.remove_entities(emptied)? == 0 {
        return Ok(changed_layouts);
      }
    }
  }

  /// Removes each entity of `entities`, by YMAP name hash and merge identity, planning the
  /// vanilla YMAPs holding them that are not planned yet. Returns how many were removed.
  fn remove_entities(
    &mut self,
    entities: Vec<(u32, u32)>,
  ) -> Result<usize, Box<dyn std::error::Error>> {
    let mut by_map = BTreeMap::<u32, HashSet<u32>>::new();
    for (hash, id) in entities {
      by_map.entry(hash).or_default().insert(id);
    }
    let mut removed = 0;
    for (hash, ids) in by_map {
      if !self.planned.contains_key(&hash) {
        if !self.vanilla_map_paths.contains_key(&hash) {
          continue;
        }
        let name = self.vanilla_name(hash)?.to_string();
        let original = self.sources.get(None, hash)?.ok_or("vanilla parent YMAP unavailable")?;
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
      let output = self.planned.get_mut(&hash).ok_or("planned YMAP missing")?;
      let before = output.entities().len();
      let mut guids = Vec::new();
      output.retain_entities(|entity| {
        let keep = !ids.contains(&entity.guid);
        if !keep {
          guids.push(entity.guid);
        }
        keep
      });
      removed += before - output.entities().len();
      for id in guids {
        self.references.record_removed(id);
        tracing::debug!(
          "    Entity {} in {} has no children left; removing it",
          self.references.original_guid(id)?,
          output.name
        );
      }
    }
    Ok(removed)
  }
}
