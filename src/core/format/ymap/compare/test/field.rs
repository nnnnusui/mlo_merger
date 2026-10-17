use super::*;
use crate::core::format::ymap::compare::PositionDiff;

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
