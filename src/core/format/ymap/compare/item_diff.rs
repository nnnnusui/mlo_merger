//! Differences of one item in a YMAP array, shared by every array that pairs its items.
//!
//! Removed, replaced and modified items are named by their vanilla index, and added items by the
//! vanilla item they follow, so applying keeps the kept items in vanilla order and diffs of
//! several sources against one vanilla array can be combined without their indices conflicting.

use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use serde::{Deserialize, Serialize};
use struct_diff::{ApplyError, Diff};

use super::field::{Pairing, anchors};

/// How one array item of a modded YMAP differs from vanilla.
///
/// `T` is the item and `C` its changed fields, usually `T`'s [`Diff::Output`]; arrays compared
/// as multisets use [`NoFieldDiff`], so they are never `Modified`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum YmapItemDiff<T, C> {
  /// Present only in the modded YMAP, right after vanilla item `after`, or first when `None`.
  Added {
    after: Option<usize>,
    item: T,
  },
  /// Present only in vanilla, at `index`.
  Removed {
    index: usize,
  },
  /// Vanilla item `index` is unpaired and the modded YMAP has `item`, also unpaired, in its place.
  Replaced {
    index: usize,
    item: T,
  },
  /// Paired with a modded item and differing beyond the tolerances. `modded_index` records where
  /// the source has it; applying keeps the item at its vanilla place.
  Modified {
    vanilla_index: usize,
    modded_index: usize,
    changes: C,
  },
}

/// Changes of items that are only ever added, removed or replaced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum NoFieldDiff {}

impl<T: Clone, C> YmapItemDiff<T, C> {
  /// The unpaired items: removed vanilla items, those replaced in place, then added `modded`
  /// items.
  ///
  /// Between two kept items, the unpaired vanilla and modded items are matched in order; matched
  /// ones are replacements, and the rest are removals or additions following the last
  /// replacement.
  pub(super) fn unpaired(
    pairing: Pairing,
    modded: &[T],
  ) -> Vec<Self> {
    let added_anchors = anchors(&pairing.pairs, modded.len());
    let kept =
      pairing.pairs.iter().map(|(vanilla_index, _)| *vanilla_index).collect::<BTreeSet<_>>();
    let mut gaps = BTreeMap::<Option<usize>, (Vec<usize>, Vec<usize>)>::new();
    let mut removed = pairing.removed;
    removed.sort_unstable();
    for index in removed {
      gaps.entry(kept.range(..index).next_back().copied()).or_default().0.push(index);
    }
    let mut added = pairing.added;
    added.sort_unstable();
    for index in added {
      gaps.entry(added_anchors[index]).or_default().1.push(index);
    }
    let (mut removals, mut replacements, mut additions) = (Vec::new(), Vec::new(), Vec::new());
    for (anchor, (removed, added)) in gaps {
      let replaced = removed.len().min(added.len());
      replacements.extend(removed.iter().zip(&added).map(|(&index, &modded_index)| {
        Self::Replaced {
          index,
          item: modded[modded_index].clone(),
        }
      }));
      removals.extend(removed[replaced..].iter().map(|&index| Self::Removed {
        index,
      }));
      let after = replaced.checked_sub(1).map_or(anchor, |last| Some(removed[last]));
      additions.extend(added[replaced..].iter().map(|&index| Self::Added {
        after,
        item: modded[index].clone(),
      }));
    }
    removals.into_iter().chain(replacements).chain(additions).collect()
  }

  /// Pairs whose `changes(vanilla_index, modded_index)` are not `None`, then the unpaired items.
  pub(super) fn collect_with(
    pairing: Pairing,
    modded: &[T],
    changes: impl Fn(usize, usize) -> Option<C>,
  ) -> Vec<Self> {
    let mut diffs = Vec::new();
    for &(vanilla_index, modded_index) in &pairing.pairs {
      if let Some(changes) = changes(vanilla_index, modded_index) {
        diffs.push(Self::Modified {
          vanilla_index,
          modded_index,
          changes,
        });
      }
    }
    diffs.extend(Self::unpaired(pairing, modded));
    diffs
  }

  /// Rebuilds `items` from these `diffs`, changing modified items with `modify`; see
  /// [`apply_changes`].
  pub(super) fn apply_with(
    items: &mut Vec<T>,
    diffs: &[Self],
    modify: impl Fn(&mut T, &C) -> Result<(), ApplyError>,
  ) -> Result<(), ApplyError> {
    *items = apply_changes(items, diffs.iter().map(Self::change), modify)?;
    Ok(())
  }
}

impl<T: Diff + Clone> YmapItemDiff<T, T::Output> {
  /// Pairs that differ under `ctx`, then the unpaired items.
  pub(super) fn collect(
    pairing: Pairing,
    vanilla: &[T],
    modded: &[T],
    ctx: &T::Context,
  ) -> Vec<Self> {
    Self::collect_with(pairing, modded, |vanilla_index, modded_index| {
      vanilla[vanilla_index].diff(&modded[modded_index], ctx)
    })
  }

  /// Rebuilds `items` from these `diffs`, applying each item's changes with [`Diff::apply`].
  pub(super) fn apply(
    items: &mut Vec<T>,
    diffs: &[Self],
  ) -> Result<(), ApplyError> {
    Self::apply_with(items, diffs, T::apply)
  }
}

impl<T: Clone> YmapItemDiff<T, NoFieldDiff> {
  /// Rebuilds `items` from these `diffs`, which only add, remove and replace items.
  pub(super) fn apply_unpaired(
    items: &mut Vec<T>,
    diffs: &[Self],
  ) -> Result<(), ApplyError> {
    Self::apply_with(items, diffs, |_, changes| match *changes {})
  }
}

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
    Self::collect_with(changes, |vanilla_index, _, _| {
      tracing::info!(
        "    {what}: vanilla item {vanilla_index} is already modified; keeping the first change"
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
          after,
          item,
        } => {
          merged.added.push((after, item.clone()));
          continue;
        }
        ItemChange::Removed {
          index,
        } => {
          merged.removed.insert(index);
          continue;
        }
        ItemChange::Replaced {
          index,
          item,
        } => (index, InPlace::Replaced(item)),
        ItemChange::Modified {
          vanilla_index,
          changes,
          ..
        } => (vanilla_index, InPlace::Modified(changes)),
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
  pub(super) fn into_changes(
    self,
    vanilla_len: usize,
  ) -> impl Iterator<Item = MergedChange<T, C>> {
    // Where each kept vanilla item ends up, for the record in `Modified`.
    let mut following = BTreeMap::<Option<usize>, usize>::new();
    for (after, _) in &self.added {
      *following.entry(*after).or_default() += 1;
    }
    let mut position = following.get(&None).copied().unwrap_or(0);
    let mut placed = BTreeMap::new();
    for index in 0..vanilla_len {
      if !self.removed.contains(&index) {
        placed.insert(index, position);
        position += 1;
      }
      position += following.get(&Some(index)).copied().unwrap_or(0);
    }
    let removed = self.removed.into_iter().map(|index| MergedChange::Removed {
      index,
    });
    let in_place = self.in_place.into_iter().map(move |(index, change)| match change {
      InPlace::Replaced(item) => MergedChange::Replaced {
        index,
        item,
      },
      InPlace::Modified(changes) => MergedChange::Modified {
        vanilla_index: index,
        // Out-of-range vanilla indices are left for applying to reject.
        modded_index: placed.get(&index).copied().unwrap_or(index),
        changes,
      },
    });
    let added = self.added.into_iter().map(|(after, item)| MergedChange::Added {
      after,
      item,
    });
    removed.chain(in_place).chain(added)
  }
}

/// One change of a [`MergedItems`] diff, owning its item and changes.
pub(super) enum MergedChange<T, C> {
  Added {
    after: Option<usize>,
    item: T,
  },
  Removed {
    index: usize,
  },
  Replaced {
    index: usize,
    item: T,
  },
  Modified {
    vanilla_index: usize,
    modded_index: usize,
    changes: C,
  },
}

impl<T: Clone, C: Clone> YmapItemDiff<T, C> {
  /// The changes of these `diffs`, one list per source, combined as [`MergedItems`] describes.
  pub(super) fn merged(
    what: &str,
    sources: &[&[Self]],
  ) -> MergedItems<T, C> {
    MergedItems::collect(what, sources.iter().flat_map(|diffs| diffs.iter().map(Self::change)))
  }

  /// [`Self::merged`], folding later modifications of a vanilla item in with `combine`; see
  /// [`MergedItems::collect_with`].
  pub(super) fn merged_with(
    sources: &[&[Self]],
    combine: impl FnMut(usize, &mut C, &C),
  ) -> MergedItems<T, C> {
    MergedItems::collect_with(
      sources.iter().flat_map(|diffs| diffs.iter().map(Self::change)),
      combine,
    )
  }

  /// A diff of `vanilla_len` items making the `merged` changes.
  pub(super) fn from_merged(
    merged: MergedItems<T, C>,
    vanilla_len: usize,
  ) -> Vec<Self> {
    merged
      .into_changes(vanilla_len)
      .map(|change| match change {
        MergedChange::Added {
          after,
          item,
        } => Self::Added {
          after,
          item,
        },
        MergedChange::Removed {
          index,
        } => Self::Removed {
          index,
        },
        MergedChange::Replaced {
          index,
          item,
        } => Self::Replaced {
          index,
          item,
        },
        MergedChange::Modified {
          vanilla_index,
          modded_index,
          changes,
        } => Self::Modified {
          vanilla_index,
          modded_index,
          changes,
        },
      })
      .collect()
  }
}

impl<T, C> YmapItemDiff<T, C> {
  /// This diff as an [`ItemChange`].
  fn change(&self) -> ItemChange<'_, T, C> {
    match self {
      Self::Added {
        after,
        item,
      } => ItemChange::Added {
        after: *after,
        item,
      },
      Self::Removed {
        index,
      } => ItemChange::Removed {
        index: *index,
      },
      Self::Replaced {
        index,
        item,
      } => ItemChange::Replaced {
        index: *index,
        item,
      },
      Self::Modified {
        vanilla_index,
        modded_index,
        changes,
      } => ItemChange::Modified {
        vanilla_index: *vanilla_index,
        modded_index: *modded_index,
        changes,
      },
    }
  }
}

/// One change to an array item, however its diff spells it.
pub(super) enum ItemChange<'a, T, C> {
  Added {
    after: Option<usize>,
    item: &'a T,
  },
  Removed {
    index: usize,
  },
  Replaced {
    index: usize,
    item: &'a T,
  },
  Modified {
    vanilla_index: usize,
    #[allow(dead_code)]
    modded_index: usize,
    changes: &'a C,
  },
}

/// What applying does to one vanilla item.
enum Applied<'a, T, C> {
  Removed,
  Replaced(&'a T),
  Modified(&'a C),
}

/// The modded array rebuilt from `vanilla` and its item `changes`.
///
/// Every vanilla item is tagged with its index; removed items are dropped, replaced and modified
/// ones changed in place, and added items follow the vanilla item they name, in the order given,
/// whether or not that item is removed. Kept items therefore stay in vanilla order.
pub(super) fn apply_changes<'a, T: Clone + 'a, C: 'a>(
  vanilla: &[T],
  changes: impl IntoIterator<Item = ItemChange<'a, T, C>>,
  modify: impl Fn(&mut T, &C) -> Result<(), ApplyError>,
) -> Result<Vec<T>, ApplyError> {
  let len = vanilla.len();
  let out_of_range =
    |index: usize| ApplyError::new(format!("vanilla index {index} is out of {len} items"));
  let mut applied = (0..len).map(|_| None::<Applied<'a, T, C>>).collect::<Vec<_>>();
  let mut record = |index: usize, change: Applied<'a, T, C>| match applied.get_mut(index) {
    None => Err(out_of_range(index)),
    Some(Some(_)) => Err(ApplyError::new(format!("vanilla index {index} is changed twice"))),
    Some(slot) => {
      *slot = Some(change);
      Ok(())
    }
  };
  // Additions before the first vanilla item, then after each one.
  let mut following = vec![Vec::new(); len + 1];
  for change in changes {
    match change {
      ItemChange::Added {
        after,
        item,
      } => {
        let slot = match after {
          None => 0,
          Some(index) if index < len => index + 1,
          Some(index) => return Err(out_of_range(index)),
        };
        following[slot].push(item);
      }
      ItemChange::Removed {
        index,
      } => record(index, Applied::Removed)?,
      ItemChange::Replaced {
        index,
        item,
      } => record(index, Applied::Replaced(item))?,
      ItemChange::Modified {
        vanilla_index,
        changes,
        ..
      } => record(vanilla_index, Applied::Modified(changes))?,
    }
  }
  let mut items = following[0].iter().map(|item| (*item).clone()).collect::<Vec<_>>();
  for (index, item) in vanilla.iter().enumerate() {
    match &applied[index] {
      None => items.push(item.clone()),
      Some(Applied::Removed) => {}
      Some(Applied::Replaced(replacement)) => items.push((*replacement).clone()),
      Some(Applied::Modified(changes)) => {
        let mut item = item.clone();
        modify(&mut item, changes)?;
        items.push(item);
      }
    }
    items.extend(following[index + 1].iter().map(|item| (*item).clone()));
  }
  Ok(items)
}
