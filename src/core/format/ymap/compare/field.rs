//! Shared helpers for pairing YMAP array items and thresholding their field changes.

use std::{
  collections::{HashMap, VecDeque},
  hash::Hash,
};

use super::PositionDiff;
use crate::common::position::Position;

/// `to` when it differs from `from` by at least `tolerance`.
pub(super) fn changed_f32(
  from: f32,
  to: f32,
  tolerance: f32,
) -> Option<f32> {
  ((from - to).abs() >= tolerance).then_some(to)
}

/// `to` when it differs from `from`.
pub(super) fn changed<T: PartialEq + Clone>(
  from: &T,
  to: &T,
) -> Option<T> {
  (from != to).then(|| to.clone())
}

impl PositionDiff {
  /// Axes of `after` differing from `before` by at least `tolerance`, or `None` if none do.
  pub fn between(
    before: &Position,
    after: &Position,
    tolerance: f32,
  ) -> Option<Self> {
    let axes = Self {
      x: changed_f32(before.x, after.x, tolerance),
      y: changed_f32(before.y, after.y, tolerance),
      z: changed_f32(before.z, after.z, tolerance),
    };
    (axes != Self::default()).then_some(axes)
  }
}

/// Vanilla and modded indices of items judged the same, plus the unpaired ones.
pub(super) struct Pairing {
  pub pairs: Vec<(usize, usize)>,
  pub removed: Vec<usize>,
  pub added: Vec<usize>,
}

/// Pairs each vanilla item with the first unpaired modded item that `is_same` accepts.
pub(super) fn pair_in_order<T>(
  vanilla: &[T],
  modded: &[T],
  is_same: impl Fn(&T, &T) -> bool,
) -> Pairing {
  let mut used = vec![false; modded.len()];
  let mut pairs = Vec::new();
  let mut removed = Vec::new();
  for (vanilla_index, item) in vanilla.iter().enumerate() {
    let found = modded
      .iter()
      .enumerate()
      .position(|(index, candidate)| !used[index] && is_same(item, candidate));
    match found {
      Some(modded_index) => {
        used[modded_index] = true;
        pairs.push((vanilla_index, modded_index));
      }
      None => removed.push(vanilla_index),
    }
  }
  let added = (0..modded.len()).filter(|index| !used[*index]).collect();
  Pairing {
    pairs,
    removed,
    added,
  }
}

/// Pairs each vanilla key with the first unpaired modded item of an equal key.
///
/// Items without a key are never paired. Equivalent to [`pair_in_order`] with key equality,
/// but linear in the number of items.
pub(super) fn pair_by_keys<K: Hash + Eq>(
  vanilla: Vec<Option<K>>,
  modded: Vec<Option<K>>,
) -> Pairing {
  let modded_len = modded.len();
  let mut queues = HashMap::<K, VecDeque<usize>>::new();
  for (index, key) in modded.into_iter().enumerate() {
    if let Some(key) = key {
      queues.entry(key).or_default().push_back(index);
    }
  }
  let mut used = vec![false; modded_len];
  let mut pairs = Vec::new();
  let mut removed = Vec::new();
  for (vanilla_index, key) in vanilla.into_iter().enumerate() {
    match key.and_then(|key| queues.get_mut(&key)?.pop_front()) {
      Some(modded_index) => {
        used[modded_index] = true;
        pairs.push((vanilla_index, modded_index));
      }
      None => removed.push(vanilla_index),
    }
  }
  let added = (0..modded_len).filter(|index| !used[*index]).collect();
  Pairing {
    pairs,
    removed,
    added,
  }
}

/// `position` scaled by `10^decimals` and rounded, identifying items at one spot.
pub(super) fn rounded_position_key(
  position: &Position,
  decimals: u32,
) -> (i64, i64, i64) {
  let scale = 10f64.powi(decimals as i32);
  let round = |value: f32| (f64::from(value) * scale).round() as i64;
  (round(position.x), round(position.y), round(position.z))
}
