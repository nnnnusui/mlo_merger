use structdiff::{Difference, StructDiff};

use crate::{
  common::{position::Position, triangle::Triangle},
  core::format::ymap::compare::{self, YmapTriangleDiff},
};

/// Occlude model definition in YMAP
#[derive(Debug, Clone, PartialEq, Difference, struct_diff::Diff)]
#[difference(expose)]
#[derive(serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[diff(
  name = YmapOccludeModelFieldDiff,
  attr(cfg_attr(feature = "typescript", derive(specta::Type)))
)]
pub struct YmapOccludeModel {
  #[diff(nested, context = 0.0)]
  pub bmin: Position,
  #[diff(nested, context = 0.0)]
  pub bmax: Position,
  #[diff(with = compare::occlude_models::triangles, output = Vec<YmapTriangleDiff>)]
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
      |p1: &crate::common::position::Position, p2: &crate::common::position::Position| {
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
