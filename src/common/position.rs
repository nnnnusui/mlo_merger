#[derive(
  Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize, struct_diff::Diff,
)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[diff(
  context = f32,
  attr(cfg_attr(feature = "typescript", derive(specta::Type)))
)]
pub struct Position {
  #[diff(threshold = *ctx)]
  pub x: f32,
  #[diff(threshold = *ctx)]
  pub y: f32,
  #[diff(threshold = *ctx)]
  pub z: f32,
}

impl Position {
  /// The position scaled by `10^decimals` and rounded, identifying items at one spot.
  pub(crate) fn rounded_key(
    &self,
    decimals: u32,
  ) -> (i64, i64, i64) {
    let scale = 10f64.powi(decimals as i32);
    let round = |value: f32| (f64::from(value) * scale).round() as i64;
    (round(self.x), round(self.y), round(self.z))
  }
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
