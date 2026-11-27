//! Leaving out what a source keeps from the older vanilla version it was made from.

use super::*;
use crate::common::triangle::Triangle;
use crate::core::format::ymap::compare::baseline::{
  BaselineComparison, change_count, relative_to_baseline,
};
use crate::core::format::ymap::compare::{ItemDiff, YmapEntityDiff};
use crate::core::format::ymap::model::{
  BoundingBox, GrassInstance, GrassInstanceBatch, Vector3, Vector4, YmapBoxOccluder, YmapLodLight,
  YmapOccludeModel,
};

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
  relative_in(latest, baseline, source, false)
}

/// [`relative`] for a YMAP of a LOD light pair or not.
fn relative_in(
  latest: &Ymap,
  baseline: &Ymap,
  source: &Ymap,
  light_pair: bool,
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
      light_pair,
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

#[test]
fn occlude_triangles_relate_to_the_baseline_by_their_vertices() {
  let triangle = |x: f32| Triangle {
    corner_1: at(x, 0.0, 0.0),
    corner_2: at(x + 1.0, 0.0, 0.0),
    corner_3: at(x, 1.0, 0.0),
  };
  let occluding = |xs: &[f32]| Ymap {
    occlude_models: vec![YmapOccludeModel {
      bmin: at(0.0, 0.0, 0.0),
      bmax: at(0.0, 0.0, 0.0),
      triangles: xs.iter().map(|x| triangle(*x)).collect(),
      flags: 0,
    }],
    ..base()
  };
  // Vanilla later added 2 and removed 4; the source cut 1 out and added 3.
  let baseline = occluding(&[0.0, 1.0, 4.0]);
  let latest = occluding(&[0.0, 1.0, 2.0]);
  let source = occluding(&[0.0, 3.0, 4.0]);
  let diffs = relative(&latest, &baseline, &source).occlude_models.unwrap();
  assert_eq!(diffs.len(), 1);
  assert_eq!(diffs[0].removed, [triangle(1.0)]);
  assert_eq!(diffs[0].added, [triangle(3.0)]);
}

#[test]
fn physics_dictionaries_relate_to_the_baseline_by_name() {
  let named = |names: &[&str]| Ymap {
    physics_dictionaries: names.iter().map(|name| (*name).to_owned()).collect(),
    ..base()
  };
  let baseline = named(&["a", "b"]);
  let latest = named(&["a", "c"]);
  let source = named(&["a", "b", "d"]);
  let diffs = relative(&latest, &baseline, &source).physics_dictionaries.unwrap();
  // Only `d`: `c` came after the baseline and `b` left the latest.
  assert_eq!(diffs.len(), 1, "{diffs:?}");
  assert!(matches!(&diffs[0], ItemDiff::Added { item, .. } if item == "d"));
}

fn grass_batch(
  max_x: f32,
  xs: &[f32],
) -> GrassInstanceBatch {
  let corner = |x: f32, y: f32| Vector4 {
    x,
    y,
    z: 0.0,
    w: 0.0,
  };
  GrassInstanceBatch {
    batch_aabb: BoundingBox {
      min: corner(0.0, 0.0),
      max: corner(max_x, 1.0),
    },
    scale_range: Vector3 {
      x: 1.0,
      y: 1.0,
      z: 1.0,
    },
    archetype_name: "grass".into(),
    lod_dist: 100.0,
    lod_fade_start_dist: 50.0,
    lod_inst_fade_range: 10.0,
    orient_to_terrain: 1,
    instances: xs
      .iter()
      .map(|x| GrassInstance {
        position: [(x / max_x * 65536.0).round(), 0.0, 0.0],
        normal_x: 0,
        normal_y: 0,
        color: [255, 255, 255],
        scale: 1,
        ao: 0,
        pad: [0, 0, 0],
      })
      .collect(),
  }
}

#[test]
fn grass_instances_relate_to_the_baseline_and_keep_the_latest_bounds() {
  let grass = |batch: GrassInstanceBatch| Ymap {
    instanced_data: YmapInstancedData {
      grass_instance_list: vec![batch],
      ..Default::default()
    },
    ..base()
  };
  // Vanilla later grew the batch for an instance at 15; the source removed 2 and added 5.
  let baseline = grass(grass_batch(10.0, &[1.0, 2.0, 3.0]));
  let latest = grass(grass_batch(20.0, &[1.0, 2.0, 3.0, 15.0]));
  let source = grass(grass_batch(10.0, &[1.0, 3.0, 5.0]));
  let instanced = relative(&latest, &baseline, &source).instanced_data.unwrap();
  let batches = instanced.grass_instance_list.unwrap();
  let [
    ItemDiff::Modified {
      changes,
      ..
    },
  ] = batches.as_slice()
  else {
    panic!("{batches:?}");
  };
  assert_eq!(changes.removed_instances, [1]);
  assert!(changes.modified_instances.is_empty());
  // The latest bounds stay, and the added instance is packed into them.
  assert_eq!(changes.batch_aabb, None);
  let [added] = changes.added_instances.as_slice() else {
    panic!("{changes:?}");
  };
  assert_eq!(added.instance.position[0], (5.0f32 / 20.0 * 65536.0).round());
}

#[test]
fn the_lights_of_a_lod_light_pair_stay_as_taken_from_the_latest() {
  let lights = |falloffs: &[f32]| Ymap {
    lod_lights: falloffs
      .iter()
      .map(|falloff| YmapLodLight {
        falloff: *falloff,
        hash: "0".into(),
        ..YmapLodLight::default()
      })
      .collect(),
    ..base()
  };
  let (baseline, latest) = (lights(&[1.0]), lights(&[1.0, 2.0]));
  // The light vanilla added later is not removed from a lone YMAP...
  assert!(relative_in(&latest, &baseline, &baseline, false).lod_lights.is_none());
  // ...but stays removed from one of a pair, whose partner removes it too.
  assert!(relative_in(&latest, &baseline, &baseline, true).lod_lights.is_some());
}
