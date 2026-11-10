//! Combining the array diffs of several sources from one vanilla array into one diff.

use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use serde::{Deserialize, Serialize};
use struct_diff::vec::{ItemChange, ItemDiff};

/// Changes of items that are only ever added, removed or replaced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum NoFieldDiff {}

/// What one source does to one vanilla item it keeps a place for.
enum InPlace<T, C> {
  Modified(C),
  Replaced(T),
}

/// The changes several sources make to one vanilla array, combined: a removal wins over every
/// other change of the vanilla item, including a replacement; other changes of one vanilla item
/// keep the first source's, except modifications, which combine as [`Self::collect_with`] is
/// told; additions after one vanilla item are kept in source order.
pub(super) struct MergedItems<T, C> {
  removed: BTreeSet<usize>,
  in_place: BTreeMap<usize, InPlace<T, C>>,
  added: Vec<(Option<usize>, T)>,
}

impl<T, C> MergedItems<T, C> {
  /// Collects `changes` in source order, keeping the first change of each vanilla item; `what`
  /// names the array in logs.
  pub(super) fn collect<'a>(
    what: &str,
    changes: impl IntoIterator<Item = ItemChange<'a, T, C>>,
  ) -> Self
  where
    T: Clone + 'a,
    C: Clone + 'a,
  {
    Self::collect_with(changes, |index, _, _| {
      tracing::info!(
        "    {what}: vanilla item {index} is already modified; keeping the first change"
      );
    })
  }

  /// Collects `changes` in source order, letting `combine(vanilla_index, earlier, later)` fold a
  /// later modification of a vanilla item into the earlier ones.
  pub(super) fn collect_with<'a>(
    changes: impl IntoIterator<Item = ItemChange<'a, T, C>>,
    mut combine: impl FnMut(usize, &mut C, &C),
  ) -> Self
  where
    T: Clone + 'a,
    C: Clone + 'a,
  {
    let mut merged = Self {
      removed: BTreeSet::new(),
      in_place: BTreeMap::new(),
      added: Vec::new(),
    };
    for change in changes {
      let (index, change) = match change {
        ItemChange::Added {
          anchor_base_index,
          item,
        } => {
          merged.added.push((anchor_base_index, item.clone()));
          continue;
        }
        ItemChange::Removed {
          base_index,
        } => {
          merged.removed.insert(base_index);
          continue;
        }
        ItemChange::Replaced {
          base_index,
          item,
        } => (base_index, InPlace::Replaced(item)),
        ItemChange::Modified {
          base_index,
          changes,
        } => (base_index, InPlace::Modified(changes)),
      };
      match (merged.in_place.entry(index), change) {
        (Entry::Vacant(entry), InPlace::Modified(changes)) => {
          entry.insert(InPlace::Modified(changes.clone()));
        }
        (Entry::Vacant(entry), InPlace::Replaced(item)) => {
          entry.insert(InPlace::Replaced(item.clone()));
        }
        (Entry::Occupied(mut entry), InPlace::Modified(changes)) => {
          if let InPlace::Modified(earlier) = entry.get_mut() {
            combine(index, earlier, changes);
          } else {
            tracing::info!("    vanilla item {index} is already replaced; keeping the replacement");
          }
        }
        (Entry::Occupied(_), InPlace::Replaced(_)) => {
          tracing::info!("    vanilla item {index} is already changed; keeping the first change");
        }
      }
    }
    let removed = &merged.removed;
    merged.in_place.retain(|index, change| {
      let dropped = removed.contains(index);
      if dropped && matches!(change, InPlace::Replaced(_)) {
        tracing::info!("    vanilla item {index} is removed; dropping its replacement");
      }
      !dropped
    });
    merged
  }

  /// Whether vanilla item `index` is removed or replaced.
  pub(super) fn drops_vanilla(
    &self,
    index: usize,
  ) -> bool {
    self.removed.contains(&index) || matches!(self.in_place.get(&index), Some(InPlace::Replaced(_)))
  }

  /// Keeps only the added and replacing items `keep` accepts, in order; a vanilla item whose
  /// replacement is dropped is removed.
  pub(super) fn retain_new(
    &mut self,
    mut keep: impl FnMut(&T) -> bool,
  ) {
    let mut dropped = Vec::new();
    for (index, change) in &self.in_place {
      if let InPlace::Replaced(item) = change
        && !keep(item)
      {
        dropped.push(*index);
      }
    }
    for index in dropped {
      self.in_place.remove(&index);
      self.removed.insert(index);
    }
    self.added.retain(|(_, item)| keep(item));
  }

  /// The changes as one diff of `vanilla_len` items.
  pub(super) fn into_diffs(
    self,
    vanilla_len: usize,
  ) -> Vec<ItemDiff<T, C>> {
    // Where each kept vanilla item and each addition ends up, for the target indices.
    let mut following = BTreeMap::<Option<usize>, Vec<usize>>::new();
    for (addition, (anchor, _)) in self.added.iter().enumerate() {
      following.entry(*anchor).or_default().push(addition);
    }
    let mut added_at = vec![None; self.added.len()];
    let mut position = 0;
    let mut place_following = |anchor: Option<usize>, position: &mut usize| {
      for &addition in following.get(&anchor).map(Vec::as_slice).unwrap_or_default() {
        added_at[addition] = Some(*position);
        *position += 1;
      }
    };
    place_following(None, &mut position);
    let mut placed = BTreeMap::new();
    for index in 0..vanilla_len {
      if !self.removed.contains(&index) {
        placed.insert(index, position);
        position += 1;
      }
      place_following(Some(index), &mut position);
    }
    let removed = self.removed.into_iter().map(|base_index| ItemDiff::Removed {
      base_index,
      implied: false,
      reason: None,
    });
    let in_place = self.in_place.into_iter().map(move |(base_index, change)| match change {
      InPlace::Replaced(item) => ItemDiff::Replaced {
        base_index,
        target_index: placed.get(&base_index).copied(),
        item,
      },
      InPlace::Modified(changes) => ItemDiff::Modified {
        base_index,
        // Out-of-range vanilla indices are left for applying to reject.
        target_index: placed.get(&base_index).copied().unwrap_or(base_index),
        changes,
        reason: None,
      },
    });
    // Additions after an out-of-range vanilla index are left for applying to reject.
    let added =
      self.added.into_iter().zip(added_at).map(|((anchor_base_index, item), at)| ItemDiff::Added {
        anchor_base_index,
        target_index: at,
        item,
      });
    removed.chain(in_place).chain(added).collect()
  }
}

/// The changes of these `diffs`, one list per source, combined as [`MergedItems`] describes.
pub(super) fn merged<T: Clone, C: Clone>(
  what: &str,
  sources: &[&[ItemDiff<T, C>]],
) -> MergedItems<T, C> {
  MergedItems::collect(what, sources.iter().flat_map(|diffs| diffs.iter().map(ItemDiff::change)))
}

/// [`merged`], folding later modifications of a vanilla item in with `combine`; see
/// [`MergedItems::collect_with`].
pub(super) fn merged_with<T: Clone, C: Clone>(
  sources: &[&[ItemDiff<T, C>]],
  combine: impl FnMut(usize, &mut C, &C),
) -> MergedItems<T, C> {
  MergedItems::collect_with(
    sources.iter().flat_map(|diffs| diffs.iter().map(ItemDiff::change)),
    combine,
  )
}
