use serde_json::json;

use super::*;
use crate::core::format::ymap::compare::*;
use crate::core::format::ymap::model::{
  BoundingBox, GrassInstance, GrassInstanceBatch, Vector3, Vector4,
};

/// An instance at world `x` (y = z = 0) packed into a batch spanning `min_x..max_x`.
fn instance(
  x: f32,
  min_x: f32,
  max_x: f32,
) -> GrassInstance {
  let packed = ((x - min_x) / (max_x - min_x) * 65536.0).round();
  GrassInstance {
    position: vec![packed, 0.0, 0.0],
    normal_x: 0,
    normal_y: 0,
    color: vec![255, 255, 255],
    scale: 1,
    ao: 0,
    pad: vec![0, 0, 0],
  }
}

fn bounds(
  min_x: f32,
  max_x: f32,
) -> BoundingBox {
  let corner = |x: f32, y: f32| Vector4 {
    x,
    y,
    z: 0.0,
    w: 0.0,
  };
  BoundingBox {
    min: corner(min_x, 0.0),
    max: corner(max_x, 1.0),
  }
}

fn batch(
  min_x: f32,
  max_x: f32,
  xs: &[f32],
) -> GrassInstanceBatch {
  GrassInstanceBatch {
    batch_aabb: bounds(min_x, max_x),
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
    instances: xs.iter().map(|x| instance(*x, min_x, max_x)).collect(),
  }
}

fn with_grass(batches: Vec<GrassInstanceBatch>) -> Ymap {
  Ymap {
    instanced_data: YmapInstancedData {
      grass_instance_list: batches,
      ..YmapInstancedData::default()
    },
    ..base()
  }
}

#[test]
fn prop_instances_and_the_imap_link_compare_by_name() {
  let vanilla = Ymap {
    instanced_data: YmapInstancedData {
      imap_link: "link".into(),
      prop_instance_list: vec!["a".into(), "b".into()],
      ..YmapInstancedData::default()
    },
    ..base()
  };
  let mut modded = vanilla.clone();
  modded.instanced_data.imap_link = "other_link".into();
  modded.instanced_data.prop_instance_list = vec!["b".into(), "c".into()];
  assert_eq!(
    json(&vanilla, &modded),
    json!({
      "instanced_data": {
        "imap_link": "other_link",
        "prop_instance_list": [
          { "Removed": { "index": 0 } },
          { "Added": { "after": 1, "item": "c" } },
        ],
      },
    })
  );
}

#[test]
fn grass_batches_compare_instances_in_world_space() {
  let vanilla = with_grass(vec![batch(0.0, 100.0, &[10.0, 20.0, 30.0])]);
  // Re-bounded: 10 and 30 stay put, 20 is rescaled, and 25 is new.
  let mut modded = with_grass(vec![batch(5.0, 40.0, &[10.0, 20.0, 30.0, 25.0])]);
  modded.instanced_data.grass_instance_list[0].instances[1].scale = 2;
  let diffs = json(&vanilla, &modded);
  let changes = &diffs["instanced_data"]["grass_instance_list"][0]["Modified"]["changes"];
  assert_eq!(changes["batch_aabb"], serde_json::to_value(bounds(5.0, 40.0)).unwrap());
  assert_eq!(changes.get("removed_instances"), None);
  assert_eq!(changes["added_instances"][0]["after"], json!(2));
  assert_eq!(changes["modified_instances"][0]["vanilla_index"], json!(1));
  assert_eq!(changes["modified_instances"][0]["changes"], json!({ "scale": 2 }));
  assert_eq!(diffs["instanced_data"]["grass_instance_list"].as_array().unwrap().len(), 1);
}

#[test]
fn repainted_grass_pairs_batches_by_overlapping_bounds() {
  let vanilla = with_grass(vec![batch(0.0, 100.0, &[10.0, 20.0])]);
  let modded = with_grass(vec![batch(500.0, 600.0, &[550.0]), batch(5.0, 95.0, &[60.0])]);
  assert_eq!(
    json(&vanilla, &modded)["instanced_data"]["grass_instance_list"]
      .as_array()
      .unwrap()
      .iter()
      .map(|diff| diff.as_object().unwrap().keys().next().unwrap().clone())
      .collect::<Vec<_>>(),
    vec!["Modified", "Added"]
  );
  let diffs = diff(&vanilla, &modded);
  let grass = &diffs.instanced_data.unwrap().grass_instance_list.unwrap();
  assert!(matches!(
    &grass[0],
    crate::core::format::ymap::compare::YmapGrassBatchDiff::Modified { vanilla_index: 0, modded_index: 1, changes }
      if changes.removed_instances == vec![0, 1] && changes.added_instances.len() == 1
  ));
}

#[test]
fn grass_follows_the_configured_position_tolerance() {
  let vanilla = with_grass(vec![batch(0.0, 100.0, &[10.0])]);
  let modded = with_grass(vec![batch(0.0, 100.0, &[10.25])]);
  assert!(diff(&vanilla, &modded).instanced_data.is_some());
  let loose = MatchTolerances {
    ymap_grass_position: 0.5,
    ..Default::default()
  };
  assert_eq!(diff_with(&vanilla, &modded, &loose).instanced_data, None);
}

// Moved from `ymap_instanced_data_diff.rs`: the diff function on its own.

fn grass(
  vanilla: &Ymap,
  modded: &Ymap,
) -> Vec<YmapGrassBatchDiff> {
  vanilla
    .instanced_data
    .diff(&modded.instanced_data, &MatchTolerances::default())
    .and_then(|diff| diff.grass_instance_list)
    .unwrap_or_default()
}

#[test]
fn rebounded_batches_report_only_their_bounds() {
  let vanilla = with_grass(vec![batch(0.0, 100.0, &[10.0, 20.0])]);
  let modded = with_grass(vec![batch(5.0, 40.0, &[10.0, 20.0])]);
  assert_eq!(
    grass(&vanilla, &modded),
    vec![YmapGrassBatchDiff::Modified {
      vanilla_index: 0,
      modded_index: 0,
      changes: YmapGrassBatchFieldDiff {
        batch_aabb: Some(bounds(5.0, 40.0)),
        ..Default::default()
      },
    }]
  );
}

#[test]
fn split_batches_keep_the_larger_share_and_add_the_rest() {
  let vanilla = with_grass(vec![batch(0.0, 100.0, &[10.0, 20.0, 30.0, 70.0])]);
  let modded = with_grass(vec![batch(50.0, 80.0, &[70.0]), batch(5.0, 40.0, &[10.0, 20.0, 30.0])]);
  let diffs = grass(&vanilla, &modded);
  assert_eq!(diffs.len(), 2, "{diffs:?}");
  let YmapGrassBatchDiff::Modified {
    modded_index: 1,
    changes,
    ..
  } = &diffs[0]
  else {
    panic!("{diffs:?}");
  };
  assert_eq!(changes.removed_instances, vec![3]);
  assert!(matches!(
    &diffs[1],
    YmapGrassBatchDiff::Added {
      after: None,
      ..
    }
  ));
}

#[test]
fn repainted_batches_pair_by_overlapping_bounds() {
  let vanilla = with_grass(vec![batch(0.0, 100.0, &[10.0]), batch(500.0, 600.0, &[550.0])]);
  let modded = with_grass(vec![batch(510.0, 590.0, &[520.0]), batch(5.0, 95.0, &[60.0, 70.0])]);
  let diffs = grass(&vanilla, &modded);
  let pairs = diffs
    .iter()
    .map(|diff| match diff {
      YmapGrassBatchDiff::Modified {
        vanilla_index,
        modded_index,
        changes,
      } => (
        *vanilla_index,
        *modded_index,
        changes.removed_instances.len(),
        changes.added_instances.len(),
      ),
      other => panic!("{other:?}"),
    })
    .collect::<Vec<_>>();
  assert_eq!(pairs, vec![(0, 1, 1, 2), (1, 0, 1, 1)]);
}

#[test]
fn reports_attribute_changes_and_whole_batches() {
  let vanilla =
    with_grass(vec![batch(0.0, 100.0, &[10.0, 20.0, 30.0]), batch(200.0, 300.0, &[250.0])]);
  let mut modded =
    with_grass(vec![batch(0.0, 100.0, &[10.0, 25.0, 30.0]), batch(400.0, 500.0, &[450.0])]);
  modded.instanced_data.grass_instance_list[0].instances[0].scale = 2;
  modded.instanced_data.prop_instance_list = vec!["prop".into()];
  let diff =
    vanilla.instanced_data.diff(&modded.instanced_data, &MatchTolerances::default()).unwrap();
  assert_eq!(
    diff.prop_instance_list.unwrap(),
    vec![YmapPropInstanceDiff::Added {
      after: None,
      item: "prop".into(),
    }]
  );
  let diffs = diff.grass_instance_list.unwrap();
  assert_eq!(diffs.len(), 2, "{diffs:?}");
  let YmapGrassBatchDiff::Modified {
    vanilla_index: 0,
    modded_index: 0,
    changes,
  } = &diffs[0]
  else {
    panic!("{diffs:?}");
  };
  assert_eq!(changes.batch_aabb, None);
  assert_eq!(changes.removed_instances, vec![1]);
  assert_eq!(changes.added_instances.iter().map(|it| it.after).collect::<Vec<_>>(), vec![Some(0)]);
  assert!((changes.added_instances[0].world_position.x - 25.0).abs() < 0.01);
  assert_eq!(
    changes.modified_instances,
    vec![YmapModifiedGrassInstance {
      vanilla_index: 0,
      modded_index: 0,
      world_position: changes.modified_instances[0].world_position.clone(),
      changes: YmapGrassInstanceFieldDiff {
        scale: Some(2),
        ..Default::default()
      },
    }]
  );
  assert!(matches!(
    diffs[1],
    YmapGrassBatchDiff::Replaced {
      index: 1,
      ..
    }
  ));
}

#[test]
fn requantised_positions_within_one_packed_step_match() {
  // A 10 km batch has ~15 cm packed steps, so a re-encoded position may move by one step.
  let vanilla = with_grass(vec![batch(0.0, 10000.0, &[5000.0])]);
  let modded = with_grass(vec![batch(0.0, 10000.0, &[5000.15])]);
  assert!(grass(&vanilla, &modded).is_empty());
  let moved = with_grass(vec![batch(0.0, 10000.0, &[5000.5])]);
  assert!(!grass(&vanilla, &moved).is_empty());
}

#[test]
fn ignores_padding_and_changes_within_tolerance() {
  let vanilla = with_grass(vec![batch(0.0, 100.0, &[10.0])]);
  let mut modded = with_grass(vec![batch(0.0, 100.0, &[10.005])]);
  modded.instanced_data.grass_instance_list[0].instances[0].pad = vec![7, 7, 7];
  assert!(grass(&vanilla, &modded).is_empty());
}
