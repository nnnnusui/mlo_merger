//! Differences of one item in a YMAP array, shared by every array that pairs its items.

use serde::{Deserialize, Serialize};
use struct_diff::{ApplyError, Diff};

use super::field::Pairing;

/// How one array item of a modded YMAP differs from vanilla.
///
/// `T` is the item and `C` its changed fields, usually `T`'s [`Diff::Output`]; arrays compared
/// as multisets use [`NoFieldDiff`], so they are never `Modified`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum YmapItemDiff<T, C> {
  /// Present only in the modded YMAP, at `index`.
  Added {
    index: usize,
    item: T,
  },
  /// Present only in vanilla, at `index`.
  Removed {
    index: usize,
  },
  /// Paired with a modded item and differing beyond the tolerances.
  Modified {
    vanilla_index: usize,
    modded_index: usize,
    changes: C,
  },
}

/// Changes of items that are only ever added or removed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum NoFieldDiff {}

impl<T: Clone, C> YmapItemDiff<T, C> {
  /// Removed vanilla items, then added `modded` items, ignoring the pairs.
  pub(super) fn unpaired(
    pairing: Pairing,
    modded: &[T],
  ) -> Vec<Self> {
    let removed = pairing.removed.into_iter().map(|index| Self::Removed {
      index,
    });
    let added = pairing.added.into_iter().map(|index| Self::Added {
      index,
      item: modded[index].clone(),
    });
    removed.chain(added).collect()
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
    let changes = diffs.iter().map(|diff| match diff {
      Self::Added {
        index,
        item,
      } => ItemChange::Added {
        index: *index,
        item,
      },
      Self::Removed {
        index,
      } => ItemChange::Removed {
        index: *index,
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
    });
    *items = apply_changes(items, changes, modify)?;
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
  /// Rebuilds `items` from these `diffs`, which only add and remove items.
  pub(super) fn apply_unpaired(
    items: &mut Vec<T>,
    diffs: &[Self],
  ) -> Result<(), ApplyError> {
    Self::apply_with(items, diffs, |_, changes| match *changes {})
  }
}

/// One change to an array item, however its diff spells it.
pub(super) enum ItemChange<'a, T, C> {
  Added {
    index: usize,
    item: &'a T,
  },
  Removed {
    index: usize,
  },
  Modified {
    vanilla_index: usize,
    modded_index: usize,
    changes: &'a C,
  },
}

/// The modded array rebuilt from `vanilla` and its item `changes`.
///
/// Added and modified items go to their modded index. Diffs do not record where unchanged items
/// went, so the untouched vanilla items fill the remaining slots in vanilla order.
pub(super) fn apply_changes<'a, T: Clone + 'a, C: 'a>(
  vanilla: &[T],
  changes: impl IntoIterator<Item = ItemChange<'a, T, C>>,
  modify: impl Fn(&mut T, &C) -> Result<(), ApplyError>,
) -> Result<Vec<T>, ApplyError> {
  let mut consumed = vec![false; vanilla.len()];
  let mut consume = |index: usize| -> Result<(), ApplyError> {
    match consumed.get_mut(index) {
      None => {
        Err(ApplyError::new(format!("vanilla index {index} is out of {} items", vanilla.len())))
      }
      Some(true) => Err(ApplyError::new(format!("vanilla index {index} is changed twice"))),
      Some(slot) => {
        *slot = true;
        Ok(())
      }
    }
  };
  let mut placed = Vec::new();
  for change in changes {
    match change {
      ItemChange::Added {
        index,
        item,
      } => placed.push((index, item.clone())),
      ItemChange::Removed {
        index,
      } => consume(index)?,
      ItemChange::Modified {
        vanilla_index,
        modded_index,
        changes,
      } => {
        consume(vanilla_index)?;
        let mut item = vanilla[vanilla_index].clone();
        modify(&mut item, changes)?;
        placed.push((modded_index, item));
      }
    }
  }
  let mut kept = vanilla.iter().zip(&consumed).filter(|(_, consumed)| !**consumed);
  let len = kept.clone().count() + placed.len();
  let mut slots = vec![None; len];
  for (index, item) in placed {
    match slots.get_mut(index) {
      None => return Err(ApplyError::new(format!("modded index {index} is out of {len} items"))),
      Some(Some(_)) => {
        return Err(ApplyError::new(format!("modded index {index} is filled twice")));
      }
      Some(slot) => *slot = Some(item),
    }
  }
  // Every placed index is distinct and in range, so the empty slots match the kept items.
  Ok(slots.into_iter().map(|slot| slot.unwrap_or_else(|| kept.next().unwrap().0.clone())).collect())
}
