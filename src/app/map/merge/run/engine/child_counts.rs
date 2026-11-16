//! Counting each parent entity's children in the merged YMAPs.
//!
//! Only the planned YMAPs are read: a parent's children in the YMAPs the merge leaves as they
//! are keep their vanilla count, so each parent's `numChildren` is its vanilla value changed by
//! the difference between the children the planned YMAPs give it and those their vanilla versions
//! gave it.

use super::*;
use crate::core::format::ymap::relation::is_local_parent;

/// Children counted for each parent entity, by the parent's YMAP name hash and merge identity.
type ChildCounts = HashMap<(u32, u32), u32>;

impl<'a> MergeRun<'a> {
  /// Sets every planned entity's `num_children` to the children the merge gives it, and plans
  /// the vanilla YMAPs whose entities gain or lose children though no source changes them.
  ///
  /// A YMAP whose entities moved has all its vanilla children planned, so its entities take the
  /// children counted in `child_counts` alone.
  pub(super) fn update_child_counts(
    &mut self,
    changed_layouts: &HashSet<u32>,
    child_counts: &ChildCounts,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let planned = self.planned.keys().copied().collect::<Vec<_>>();
    let vanilla_counts = self.vanilla_child_counts(&planned)?;
    let delta = |key: &(u32, u32)| {
      i64::from(child_counts.get(key).copied().unwrap_or(0))
        - i64::from(vanilla_counts.get(key).copied().unwrap_or(0))
    };
    for hash in planned {
      let vanilla = self.vanilla_child_totals(hash)?;
      let output = self.planned.get_mut(&hash).ok_or("planned YMAP missing")?;
      for entity in output.entities_mut() {
        let key = (hash, entity.guid);
        entity.num_children = if changed_layouts.contains(&hash) {
          child_counts.get(&key).copied().unwrap_or(0)
        } else {
          let total = i64::from(vanilla.get(&entity.guid).copied().unwrap_or(0)) + delta(&key);
          u32::try_from(total.max(0))?
        };
      }
    }
    // Parents in YMAPs the merge leaves as they are, by YMAP.
    let mut unplanned = BTreeMap::<u32, HashMap<u32, i64>>::new();
    for key in child_counts.keys().chain(vanilla_counts.keys()) {
      let change = delta(key);
      if change != 0 && !self.planned.contains_key(&key.0) {
        unplanned.entry(key.0).or_default().insert(key.1, change);
      }
    }
    for (hash, changes) in unplanned {
      self.plan_child_count_change(hash, &changes)?;
    }
    Ok(())
  }

  /// The children the vanilla versions of the `planned` YMAPs give each parent.
  fn vanilla_child_counts(
    &mut self,
    planned: &[u32],
  ) -> Result<ChildCounts, Box<dyn std::error::Error>> {
    let mut counts = ChildCounts::new();
    // A parent YMAP's layout is worked out once, however many children point into it.
    let mut layouts = HashMap::<u32, Option<Vec<u32>>>::new();
    for &hash in planned {
      let Some(vanilla) = self.sources.get(None, hash)? else {
        continue;
      };
      let order = runtime_entities(vanilla.entities.iter());
      let external = reference_hash(&vanilla.model.parent);
      for entity in &vanilla.entities {
        let Ok(index) = usize::try_from(entity.parent_index) else {
          continue;
        };
        let owner = if is_local_parent(entity, &order) { hash } else { external };
        if let std::collections::hash_map::Entry::Vacant(entry) = layouts.entry(owner) {
          let layout = match self.sources.get(None, owner)? {
            Some(parent) => {
              self.references.register_entities(owner, &parent.entities);
              Some(self.references.runtime_entity_ids(owner, &parent.entities)?)
            }
            None => None,
          };
          entry.insert(layout);
        }
        if let Some(id) = layouts[&owner].as_ref().and_then(|layout| layout.get(index)) {
          *counts.entry((owner, *id)).or_default() += 1;
        }
      }
    }
    Ok(counts)
  }

  /// The `num_children` of each entity of the vanilla YMAP with name hash `hash`, by merge
  /// identity; empty when vanilla has no such YMAP.
  fn vanilla_child_totals(
    &mut self,
    hash: u32,
  ) -> Result<HashMap<u32, u32>, Box<dyn std::error::Error>> {
    let Some(vanilla) = self.sources.get(None, hash)? else {
      return Ok(HashMap::new());
    };
    let ids = self.references.entity_ids(hash, &vanilla.entities)?;
    Ok(
      ids
        .into_iter()
        .zip(&vanilla.entities)
        .map(|(id, entity)| (id, entity.num_children))
        .collect(),
    )
  }

  /// Plans the vanilla YMAP with name hash `hash` as it is but for the child counts `changes`
  /// adds to its entities, by merge identity. Its parent indices stay as they are: a YMAP whose
  /// entities moved has every vanilla child planned already.
  fn plan_child_count_change(
    &mut self,
    hash: u32,
    changes: &HashMap<u32, i64>,
  ) -> Result<(), Box<dyn std::error::Error>> {
    if !self.vanilla_map_paths.contains_key(&hash) {
      return Ok(());
    }
    let name = self.vanilla_name(hash)?.to_string();
    let original = self.sources.get(None, hash)?.ok_or("vanilla parent YMAP unavailable")?;
    let ids = self.references.entity_ids(hash, &original.entities)?;
    let mut entities = original.entities.clone();
    for (entity, id) in entities.iter_mut().zip(ids) {
      entity.guid = id;
      if let Some(change) = changes.get(&id) {
        entity.num_children = u32::try_from((i64::from(entity.num_children) + change).max(0))?;
      }
    }
    tracing::info!("  Child counts of {name} changed; writing it");
    let mut model = original.model.clone();
    model.entities = entities.clone();
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
    Ok(())
  }
}
