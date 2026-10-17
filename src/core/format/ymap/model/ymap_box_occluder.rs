use crate::core::config::matching::MatchTolerances;

/// Box occluder definition in YMAP
#[derive(Debug, Clone, PartialEq, struct_diff::Diff, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[diff(
  name = YmapBoxOccluderFieldDiff,
  context = MatchTolerances,
  matching = YmapBoxOccluder::matches,
  attr(cfg_attr(feature = "typescript", derive(specta::Type)))
)]
pub struct YmapBoxOccluder {
  pub i_center_x: i32,
  pub i_center_y: i32,
  pub i_center_z: i32,
  #[diff(with = rotation, context = ctx.ymap_box_occluder_rotation)]
  pub i_cos_z: i32,
  pub i_length: u32,
  pub i_width: u32,
  pub i_height: u32,
  #[diff(with = rotation, context = ctx.ymap_box_occluder_rotation)]
  pub i_sin_z: i32,
}

/// Compares `i_cos_z`/`i_sin_z`, ignoring changes up to the inclusive tolerance.
mod rotation {
  pub use struct_diff::replace::apply;

  pub fn diff(
    before: &i32,
    after: &i32,
    tolerance: &i32,
  ) -> Option<i32> {
    ((i64::from(*before) - i64::from(*after)).abs() > i64::from(*tolerance)).then_some(*after)
  }
}

impl YmapBoxOccluder {
  /// Whether `other` is the same occluder: centers within `tolerance.ymap_box_occluder` on every
  /// axis, in stored units.
  pub fn matches(
    &self,
    other: &YmapBoxOccluder,
    tolerances: &MatchTolerances,
  ) -> bool {
    let tolerance = tolerances.ymap_box_occluder;
    (self.i_center_x - other.i_center_x).abs() < tolerance
      && (self.i_center_y - other.i_center_y).abs() < tolerance
      && (self.i_center_z - other.i_center_z).abs() < tolerance
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn configured_box_occluder_tolerance_uses_stored_integer_units() {
    let original = YmapBoxOccluder {
      i_center_x: 0,
      i_center_y: 0,
      i_center_z: 0,
      i_cos_z: 0,
      i_length: 1,
      i_width: 1,
      i_height: 1,
      i_sin_z: 0,
    };
    let mut changed = original.clone();
    changed.i_center_x = 1;
    assert!(!original.matches(&changed, &MatchTolerances::default()));
    let wider = MatchTolerances {
      ymap_box_occluder: 2,
      ..Default::default()
    };
    assert!(original.matches(&changed, &wider));
    changed.i_center_x = 2;
    assert!(!original.matches(&changed, &wider));
  }
}
