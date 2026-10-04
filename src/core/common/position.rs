use structdiff::{Difference, StructDiff};

#[derive(Clone, Debug, Default, Difference, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Position {
  pub x: f32,
  pub y: f32,
  pub z: f32,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_position_difference() {
    let pos1 = Position {
      x: 1.0,
      y: 2.5,
      z: 3.0,
    };
    let pos2 = Position {
      x: 1.0,
      y: 2.5,
      z: 3.0,
    };

    assert_eq!(pos1, pos2);
  }
}
