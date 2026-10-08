use structdiff::{Difference, StructDiff};

use crate::core::common::{position::Position, triangle::Triangle};

/// Occlude model definition in YMAP
#[derive(Debug, Clone, PartialEq, Difference)]
#[difference(expose)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct YmapOccludeModel {
  pub bmin: Position,
  pub bmax: Position,
  pub triangles: Vec<Triangle>,
  pub flags: u32,
}

impl YmapOccludeModel {
  pub fn is_same(
    &self,
    other: &YmapOccludeModel,
  ) -> bool {
    let tolerance = crate::core::config::matching::current().ymap_occlude_model;
    let is_position_similar =
      |p1: &crate::core::common::position::Position,
       p2: &crate::core::common::position::Position| {
        (p1.x - p2.x).abs() < tolerance && (p1.y - p2.y).abs() < tolerance
      };

    is_position_similar(&self.bmin, &other.bmin) || is_position_similar(&self.bmax, &other.bmax)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::config::matching::{MatchTolerances, with_tolerances};

  #[test]
  fn configured_occlude_model_tolerance_controls_xy_matching() {
    let original = YmapOccludeModel {
      bmin: Position::default(),
      bmax: Position::default(),
      triangles: Vec::new(),
      flags: 0,
    };
    let mut changed = original.clone();
    changed.bmin.x = 0.02;
    changed.bmax.x = 0.02;
    assert!(!original.is_same(&changed));
    with_tolerances(
      MatchTolerances {
        ymap_occlude_model: 0.05,
        ..Default::default()
      },
      || {
        assert!(original.is_same(&changed));
        changed.bmin.x = 0.06;
        changed.bmax.x = 0.06;
        assert!(!original.is_same(&changed));
      },
    );
  }
}
