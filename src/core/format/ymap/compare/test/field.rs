use super::*;
use crate::core::format::ymap::compare::{
  PositionDiff,
  field::{pair_by_keys, pair_in_order},
};

#[test]
fn pairs_each_modded_item_once_in_vanilla_order() {
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
fn position_changes_below_tolerance_are_dropped() {
  let before = Position::default();
  let after = Position {
    x: 0.0005,
    y: 1.0,
    z: 0.0,
  };
  assert_eq!(
    before.diff(&after, &0.001),
    Some(PositionDiff {
      y: Some(1.0),
      ..PositionDiff::default()
    })
  );
  assert_eq!(before.diff(&before, &0.001), None);
}
