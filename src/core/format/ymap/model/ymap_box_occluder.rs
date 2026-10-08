use structdiff::{Difference, StructDiff};

/// Box occluder definition in YMAP
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct YmapBoxOccluder {
  pub i_center_x: i32,
  pub i_center_y: i32,
  pub i_center_z: i32,
  pub i_cos_z: i32,
  pub i_length: u32,
  pub i_width: u32,
  pub i_height: u32,
  pub i_sin_z: i32,
}

impl YmapBoxOccluder {
  pub fn is_same(
    &self,
    other: &YmapBoxOccluder,
  ) -> bool {
    let tolerance = crate::core::config::matching::current().ymap_box_occluder;
    (self.i_center_x - other.i_center_x).abs() < tolerance
      && (self.i_center_y - other.i_center_y).abs() < tolerance
      && (self.i_center_z - other.i_center_z).abs() < tolerance
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::config::matching::{MatchTolerances, with_tolerances};

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
    assert!(!original.is_same(&changed));
    with_tolerances(
      MatchTolerances {
        ymap_box_occluder: 2,
        ..Default::default()
      },
      || {
        assert!(original.is_same(&changed));
        changed.i_center_x = 2;
        assert!(!original.is_same(&changed));
      },
    );
  }
}
