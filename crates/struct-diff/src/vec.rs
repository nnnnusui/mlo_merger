//! Differences of arrays, item by item.
//!
//! [`Diff`] for `Vec<T>` pairs the items of two arrays as `T`'s [`Pair`] decides, then records:
//!
//! - each pair whose items differ as [`ItemDiff::Modified`], with `T`'s own changes,
//! - between two kept items, unpaired items matched in order as [`ItemDiff::Replaced`],
//! - the remaining unpaired items as [`ItemDiff::Removed`] or [`ItemDiff::Added`].
//!
//! Removed, replaced and modified items are named by their index in the earlier array, and added
//! items by the earlier item they follow, so applying keeps the kept items in their earlier
//! order and differences of several arrays from one earlier array can be combined without their
//! indices conflicting. Reordering of kept items is not recorded.
//!
//! `#[derive(Diff)]` implements [`Pair`] as the struct's `#[diff(key = ...)]` or
//! `#[diff(matching = ...)]` asks, and by equality otherwise, so items are only ever added,
//! removed or replaced unless a key or a matching function lets changed items pair.

use std::{
  collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
  hash::Hash,
};

use serde::{Deserialize, Serialize};

use super::{ApplyError, Diff};

/// How one item of a later array differs from an earlier one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum ItemDiff<T, C> {
  /// Present only in the later array, right after earlier item `after`, or first when `None`.
  Added {
    after: Option<usize>,
    item: T,
  },
  /// Present only in the earlier array, at `index`.
  Removed {
    index: usize,
  },
  /// Earlier item `index` is unpaired, and the later array has `item`, also unpaired, in its
  /// place.
  Replaced {
    index: usize,
    item: T,
  },
  /// Paired and differing. `after_index` records where the later array has it; applying keeps
  /// the item at its earlier place.
  Modified {
    before_index: usize,
    after_index: usize,
    changes: C,
  },
}

/// How the items of two arrays pair up, with the context of `Self`'s [`Diff`].
pub trait Pair: Diff + Sized {
  fn pair(
    before: &[Self],
    after: &[Self],
    ctx: &Self::Context,
  ) -> Pairing;
}

/// Earlier and later indices of items judged the same, plus the unpaired ones.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pairing {
  pub pairs: Vec<(usize, usize)>,
  pub removed: Vec<usize>,
  pub added: Vec<usize>,
}

/// Pairs each earlier item with the first unpaired later item that `is_same` accepts.
pub fn pair_in_order<T>(
  before: &[T],
  after: &[T],
  is_same: impl Fn(&T, &T) -> bool,
) -> Pairing {
  let mut used = vec![false; after.len()];
  // Every later item before `first_unused` is used, so searches start there.
  let mut first_unused = 0;
  let mut pairs = Vec::new();
  let mut removed = Vec::new();
  for (before_index, item) in before.iter().enumerate() {
    let found =
      (first_unused..after.len()).find(|&index| !used[index] && is_same(item, &after[index]));
    match found {
      Some(after_index) => {
        used[after_index] = true;
        pairs.push((before_index, after_index));
        while first_unused < after.len() && used[first_unused] {
          first_unused += 1;
        }
      }
      None => removed.push(before_index),
    }
  }
  let added = (0..after.len()).filter(|index| !used[*index]).collect();
  Pairing {
    pairs,
    removed,
    added,
  }
}

/// Pairs each earlier item with the first unpaired later item of an equal key; items without a
/// key are never paired.
///
/// Equivalent to [`pair_in_order`] with key equality, but linear in the number of items.
pub fn pair_by_keys<K: Hash + Eq>(
  before: impl IntoIterator<Item = Option<K>>,
  after: impl IntoIterator<Item = Option<K>>,
) -> Pairing {
  let mut queues = HashMap::<K, VecDeque<usize>>::new();
  let mut after_len = 0;
  for (index, key) in after.into_iter().enumerate() {
    after_len += 1;
    if let Some(key) = key {
      queues.entry(key).or_default().push_back(index);
    }
  }
  let mut used = vec![false; after_len];
  let mut pairs = Vec::new();
  let mut removed = Vec::new();
  for (before_index, key) in before.into_iter().enumerate() {
    match key.and_then(|key| queues.get_mut(&key)?.pop_front()) {
      Some(after_index) => {
        used[after_index] = true;
        pairs.push((before_index, after_index));
      }
      None => removed.push(before_index),
    }
  }
  let added = (0..after_len).filter(|index| !used[*index]).collect();
  Pairing {
    pairs,
    removed,
    added,
  }
}

/// Pairs equal items, each earlier item with the first unpaired equal later item; the default
/// [`Pair`] of `#[derive(Diff)]`.
pub fn pair_equal<T: PartialEq>(
  before: &[T],
  after: &[T],
) -> Pairing {
  pair_in_order(before, after, |a, b| a == b)
}

/// For each of `after_len` later items, the earlier index of the nearest earlier later item
/// paired, or `None` when no earlier item is paired.
///
/// An unpaired later item is placed right after that earlier item, so additions keep their place
/// among the kept items whatever other changes are applied with them.
pub fn anchors(
  pairs: &[(usize, usize)],
  after_len: usize,
) -> Vec<Option<usize>> {
  let mut paired = vec![None; after_len];
  for &(before_index, after_index) in pairs {
    paired[after_index] = Some(before_index);
  }
  let mut last = None;
  paired
    .into_iter()
    .map(|before_index| {
      let anchor = last;
      last = before_index.or(last);
      anchor
    })
    .collect()
}

impl<T: Clone, C> ItemDiff<T, C> {
  /// The unpaired items: removed earlier items, those replaced in place, then added `after`
  /// items.
  ///
  /// Between two kept items, the unpaired earlier and later items are matched in order; matched
  /// ones are replacements, and the rest are removals or additions following the last
  /// replacement.
  pub fn unpaired(
    pairing: Pairing,
    after: &[T],
  ) -> Vec<Self> {
    let added_anchors = anchors(&pairing.pairs, after.len());
    let kept = pairing.pairs.iter().map(|(before_index, _)| *before_index).collect::<BTreeSet<_>>();
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
      replacements.extend(removed.iter().zip(&added).map(|(&index, &after_index)| {
        Self::Replaced {
          index,
          item: after[after_index].clone(),
        }
      }));
      removals.extend(removed[replaced..].iter().map(|&index| Self::Removed {
        index,
      }));
      let after_anchor = replaced.checked_sub(1).map_or(anchor, |last| Some(removed[last]));
      additions.extend(added[replaced..].iter().map(|&index| Self::Added {
        after: after_anchor,
        item: after[index].clone(),
      }));
    }
    removals.into_iter().chain(replacements).chain(additions).collect()
  }

  /// Pairs whose `changes(before_index, after_index)` are not `None`, then the unpaired items.
  pub fn collect_with(
    pairing: Pairing,
    after: &[T],
    changes: impl Fn(usize, usize) -> Option<C>,
  ) -> Vec<Self> {
    let mut diffs = Vec::new();
    for &(before_index, after_index) in &pairing.pairs {
      if let Some(changes) = changes(before_index, after_index) {
        diffs.push(Self::Modified {
          before_index,
          after_index,
          changes,
        });
      }
    }
    diffs.extend(Self::unpaired(pairing, after));
    diffs
  }

  /// Rebuilds `items` from these `diffs`, changing modified items with `modify`; see
  /// [`apply_changes`].
  pub fn apply_with(
    items: &mut Vec<T>,
    diffs: &[Self],
    modify: impl Fn(&mut T, &C) -> Result<(), ApplyError>,
  ) -> Result<(), ApplyError> {
    *items = apply_changes(items, diffs.iter().map(Self::change), modify)?;
    Ok(())
  }
}

impl<T, C> ItemDiff<T, C> {
  /// This diff as an [`ItemChange`].
  pub fn change(&self) -> ItemChange<'_, T, C> {
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
        before_index,
        after_index,
        changes,
      } => ItemChange::Modified {
        before_index: *before_index,
        after_index: *after_index,
        changes,
      },
    }
  }
}

impl<T: Pair + Clone> Diff for Vec<T> {
  type Context = T::Context;
  type Output = Vec<ItemDiff<T, T::Output>>;

  fn diff(
    &self,
    other: &Self,
    ctx: &Self::Context,
  ) -> Option<Self::Output> {
    let pairing = T::pair(self, other, ctx);
    let diffs = ItemDiff::collect_with(pairing, other, |before_index, after_index| {
      self[before_index].diff(&other[after_index], ctx)
    });
    (!diffs.is_empty()).then_some(diffs)
  }

  fn apply(
    &mut self,
    diff: &Self::Output,
  ) -> Result<(), ApplyError> {
    ItemDiff::apply_with(self, diff, T::apply)
  }
}

/// One change to an array item, however its diff spells it.
pub enum ItemChange<'a, T, C> {
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
    before_index: usize,
    after_index: usize,
    changes: &'a C,
  },
}

/// What applying does to one earlier item.
enum Applied<'a, T, C> {
  Removed,
  Replaced(&'a T),
  Modified(&'a C),
}

/// The later array rebuilt from `before` and its item `changes`.
///
/// Every earlier item is tagged with its index; removed items are dropped, replaced and modified
/// ones changed in place, and added items follow the earlier item they name, in the order given,
/// whether or not that item is removed. Kept items therefore stay in their earlier order.
pub fn apply_changes<'a, T: Clone + 'a, C: 'a>(
  before: &[T],
  changes: impl IntoIterator<Item = ItemChange<'a, T, C>>,
  modify: impl Fn(&mut T, &C) -> Result<(), ApplyError>,
) -> Result<Vec<T>, ApplyError> {
  let len = before.len();
  let out_of_range = |index: usize| ApplyError::new(format!("index {index} is out of {len} items"));
  let mut applied = (0..len).map(|_| None::<Applied<'a, T, C>>).collect::<Vec<_>>();
  let mut record = |index: usize, change: Applied<'a, T, C>| match applied.get_mut(index) {
    None => Err(out_of_range(index)),
    Some(Some(_)) => Err(ApplyError::new(format!("index {index} is changed twice"))),
    Some(slot) => {
      *slot = Some(change);
      Ok(())
    }
  };
  // Additions before the first earlier item, then after each one.
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
        before_index,
        changes,
        ..
      } => record(before_index, Applied::Modified(changes))?,
    }
  }
  let mut items = following[0].iter().map(|item| (*item).clone()).collect::<Vec<_>>();
  for (index, item) in before.iter().enumerate() {
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn pairs_each_later_item_once_in_earlier_order() {
    let pairing = pair_in_order(&[1, 1, 2], &[3, 1, 1, 1], |a, b| a == b);
    assert_eq!(pairing.pairs, vec![(0, 1), (1, 2)]);
    assert_eq!(pairing.removed, vec![2]);
    assert_eq!(pairing.added, vec![0, 3]);
  }

  #[test]
  fn key_pairing_matches_in_order_pairing_and_skips_missing_keys() {
    let pairing = pair_by_keys(
      vec![Some(1), Some(1), Some(2), None],
      vec![Some(3), Some(1), None, Some(1), Some(1)],
    );
    assert_eq!(pairing.pairs, vec![(0, 1), (1, 3)]);
    assert_eq!(pairing.removed, vec![2, 3]);
    assert_eq!(pairing.added, vec![0, 2, 4]);
  }

  #[test]
  fn additions_follow_their_earlier_item_even_when_it_is_removed() {
    let diffs = [
      ItemDiff::Removed {
        index: 1,
      },
      ItemDiff::Added {
        after: Some(1),
        item: 15,
      },
      ItemDiff::Added {
        after: None,
        item: -1,
      },
      ItemDiff::Replaced {
        index: 2,
        item: 22,
      },
    ];
    let mut items = vec![0, 10, 20];
    ItemDiff::<i32, ()>::apply_with(&mut items, &diffs, |_, _| Ok(())).unwrap();
    assert_eq!(items, [-1, 0, 15, 22]);
    let twice = [
      ItemDiff::<i32, ()>::Removed {
        index: 0,
      },
      ItemDiff::Removed {
        index: 0,
      },
    ];
    let error = ItemDiff::apply_with(&mut vec![0], &twice, |_, _| Ok(())).unwrap_err();
    assert_eq!(error.to_string(), "index 0 is changed twice");
  }
}
