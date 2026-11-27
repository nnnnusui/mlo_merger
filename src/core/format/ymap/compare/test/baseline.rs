//! Leaving out what a source keeps from the older vanilla version it was made from.

use super::*;
use crate::core::format::ymap::compare::baseline::{
  BaselineComparison, change_count, relative_to_baseline,
};
use crate::core::format::ymap::compare::{ItemDiff, YmapEntityDiff};
use crate::core::format::ymap::model::YmapBoxOccluder;

fn with_entities(entities: Vec<YmapEntity>) -> Ymap {
  Ymap {
    entities,
    ..base()
  }
}

fn moved(
  guid: u32,
  x: f32,
) -> YmapEntity {
  YmapEntity {
    position: at(x, 0.0, 0.0),
    ..entity(guid)
  }
}

/// The source's differences from `latest`, relative to `baseline`.
fn relative(
  latest: &Ymap,
  baseline: &Ymap,
  source: &Ymap,
) -> YmapDiffFields {
  let tolerances = MatchTolerances::default();
  let latest_ctx = YmapDiffContext::new(tolerances, (latest, None), (source, None));
  let baseline_ctx = YmapDiffContext::new(tolerances, (baseline, None), (source, None));
  let mut diffs = latest.diff(source, &latest_ctx).unwrap_or_default();
  relative_to_baseline(
    &mut diffs,
    &BaselineComparison {
      latest,
      baseline,
      source,
      latest_ctx: &latest_ctx,
      baseline_ctx: &baseline_ctx,
    },
  );
  diffs
}

#[test]
fn only_the_changes_a_source_makes_to_its_baseline_are_kept() {
  // Vanilla later removed 1, moved 2, added 3 and kept 4.
  let baseline = with_entities(vec![moved(1, 0.0), moved(2, 0.0), moved(4, 0.0)]);
  let latest = with_entities(vec![moved(2, 10.0), moved(3, 0.0), moved(4, 0.0)]);
  // The source, made from the baseline, moves 1, keeps 2, removes 4 and adds 5.
  let source = with_entities(vec![moved(1, 5.0), moved(2, 0.0), moved(5, 0.0)]);
  let diffs = relative(&latest, &baseline, &source);
  let entities = diffs.entities.unwrap();
  assert_eq!(entities.len(), 2, "{entities:?}");
  // 4, which the source removes; 3, which vanilla added later, stays.
  assert!(matches!(
    entities[0],
    YmapEntityDiff::Removed {
      base_index: 2,
      ..
    }
  ));
  // 5, which the source adds; 1, which vanilla removed later, is not added back.
  assert!(matches!(&entities[1], YmapEntityDiff::Added { entity, .. } if entity.guid == 5));
}

#[test]
fn a_modification_makes_only_the_changes_the_source_made_to_its_baseline() {
  let baseline = with_entities(vec![moved(1, 0.0)]);
  let latest = with_entities(vec![moved(1, 10.0)]);
  let source = with_entities(vec![YmapEntity {
    lod_dist: entity(1).lod_dist + 50.0,
    ..moved(1, 0.0)
  }]);
  let diffs = relative(&latest, &baseline, &source);
  let entities = diffs.entities.unwrap();
  let [
    YmapEntityDiff::Modified {
      changes,
      ..
    },
  ] = entities.as_slice()
  else {
    panic!("{entities:?}");
  };
  let changes = serde_json::to_value(changes).unwrap();
  // The latest position stays; only the LOD distance the source raised changes.
  assert!(changes.get("position").is_none(), "{changes}");
  assert!(changes.get("lod_dist").is_some(), "{changes}");
}

#[test]
fn a_source_matching_its_baseline_makes_no_changes() {
  let occluder = |x: i32| YmapBoxOccluder {
    i_center_x: x,
    i_center_y: 0,
    i_center_z: 0,
    i_cos_z: 100,
    i_length: 4,
    i_width: 4,
    i_height: 4,
    i_sin_z: 0,
  };
  let baseline = Ymap {
    box_occluders: vec![occluder(0), occluder(100)],
    ..with_entities(vec![moved(1, 0.0), moved(2, 0.0)])
  };
  let latest = Ymap {
    box_occluders: vec![occluder(0), occluder(100), occluder(200)],
    ..with_entities(vec![moved(1, 10.0), moved(3, 0.0)])
  };
  let latest_ctx =
    YmapDiffContext::new(MatchTolerances::default(), (&latest, None), (&baseline, None));
  let from_latest = latest.diff(&baseline, &latest_ctx).unwrap();
  assert!(change_count(&from_latest) > 0);
  assert!(matches!(
    from_latest.box_occluders.as_deref(),
    Some([ItemDiff::Removed {
      base_index: 2,
      ..
    }])
  ));
  let diffs = relative(&latest, &baseline, &baseline);
  assert_eq!(change_count(&diffs), 0, "{diffs:?}");
}
