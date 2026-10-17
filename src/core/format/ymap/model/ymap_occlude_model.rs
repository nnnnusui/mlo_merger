use crate::{
  common::{position::Position, triangle::Triangle},
  core::config::matching::MatchTolerances,
};

/// Occlude model definition in YMAP
#[derive(Debug, Clone, PartialEq, struct_diff::Diff, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[diff(
  name = YmapOccludeModelFieldDiff,
  context = MatchTolerances,
  matching = YmapOccludeModel::matches,
  attr(cfg_attr(feature = "typescript", derive(specta::Type)))
)]
pub struct YmapOccludeModel {
  #[diff(nested, context = 0.0)]
  pub bmin: Position,
  #[diff(nested, context = 0.0)]
  pub bmax: Position,
  #[diff(nested, context = ())]
  pub triangles: Vec<Triangle>,
  pub flags: u32,
}

impl YmapOccludeModel {
  /// Whether `other` is the same model: minimum or maximum corners within
  /// `tolerance.ymap_occlude_model` in XY.
  pub fn matches(
    &self,
    other: &YmapOccludeModel,
    tolerances: &MatchTolerances,
  ) -> bool {
    let tolerance = tolerances.ymap_occlude_model;
    let close =
      |p: &Position, q: &Position| (p.x - q.x).abs() < tolerance && (p.y - q.y).abs() < tolerance;
    close(&self.bmin, &other.bmin) || close(&self.bmax, &other.bmax)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

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
    assert!(!original.matches(&changed, &MatchTolerances::default()));
    let wider = MatchTolerances {
      ymap_occlude_model: 0.05,
      ..Default::default()
    };
    assert!(original.matches(&changed, &wider));
    changed.bmin.x = 0.06;
    changed.bmax.x = 0.06;
    assert!(!original.matches(&changed, &wider));
  }
}
