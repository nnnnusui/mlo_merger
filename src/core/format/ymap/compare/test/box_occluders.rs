use serde_json::json;

use super::*;
use crate::core::format::ymap::compare::*;
use crate::core::format::ymap::model::YmapBoxOccluder;

fn occluder(x: i32) -> YmapBoxOccluder {
  YmapBoxOccluder {
    i_center_x: x,
    i_center_y: 0,
    i_center_z: 0,
    i_cos_z: 100,
    i_length: 4,
    i_width: 4,
    i_height: 4,
    i_sin_z: 0,
  }
}

fn with_occluders(box_occluders: Vec<YmapBoxOccluder>) -> Ymap {
  Ymap {
    box_occluders,
    ..base()
  }
}

#[test]
fn box_occluders_derive_modified_removed_and_added() {
  let vanilla = with_occluders(vec![occluder(0), occluder(100), occluder(200)]);
  let mut modded = with_occluders(vec![occluder(0), occluder(100), occluder(300)]);
  modded.box_occluders[0].i_cos_z += 1; // within tolerance.ymap_box_occluder_rotation
  modded.box_occluders[1].i_sin_z = 5;
  modded.box_occluders[1].i_width = 8;
  assert_eq!(
    json(&vanilla, &modded),
    json!({
      "box_occluders": [
        { "Modified": { "vanilla_index": 1, "modded_index": 1, "changes": { "i_width": 8, "i_sin_z": 5 } } },
        { "Removed": { "index": 2 } },
        { "Added": { "index": 2, "item": serde_json::to_value(occluder(300)).unwrap() } },
      ],
    })
  );
}

#[test]
fn box_occluders_follow_the_configured_tolerances() {
  let vanilla = with_occluders(vec![occluder(0)]);
  let mut modded = vanilla.clone();
  modded.box_occluders[0].i_center_x = 2;
  modded.box_occluders[0].i_cos_z += 1;
  // By default the center moved past the pairing tolerance and the rotation is noise.
  assert_eq!(diff(&vanilla, &modded).box_occluders.unwrap_or_default().len(), 2);
  let configured = MatchTolerances {
    ymap_box_occluder: 3,
    ymap_box_occluder_rotation: 0,
    ..Default::default()
  };
  assert_eq!(
    serde_json::to_value(diff_with(&vanilla, &modded, &configured)).unwrap(),
    json!({
      "box_occluders": [{
        "Modified": { "vanilla_index": 0, "modded_index": 0, "changes": { "i_center_x": 2, "i_cos_z": 101 } },
      }],
    })
  );
}

// Moved from `ymap_box_occluder_diff.rs`: the diff function on its own.

#[test]
fn pairs_by_center_and_thresholds_rotation() {
  let vanilla = Ymap {
    box_occluders: vec![occluder(0), occluder(100), occluder(200)],
    ..base()
  };
  let mut modded = vanilla.clone();
  modded.box_occluders[0].i_cos_z += 1;
  modded.box_occluders[1].i_cos_z += 2;
  modded.box_occluders[1].i_height = 8;
  modded.box_occluders.remove(2);
  modded.box_occluders.push(occluder(300));

  let diffs = compare::box_occluders::diff(
    &vanilla.box_occluders,
    &modded.box_occluders,
    &MatchTolerances::default(),
  )
  .unwrap();
  assert_eq!(
    diffs,
    vec![
      YmapBoxOccluderDiff::Modified {
        vanilla_index: 1,
        modded_index: 1,
        changes: YmapBoxOccluderFieldDiff {
          i_cos_z: Some(102),
          i_height: Some(8),
          ..Default::default()
        },
      },
      YmapBoxOccluderDiff::Removed {
        index: 2
      },
      YmapBoxOccluderDiff::Added {
        index: 2,
        item: occluder(300),
      },
    ]
  );
  let exact = MatchTolerances {
    ymap_box_occluder_rotation: 0,
    ..Default::default()
  };
  assert_eq!(
    compare::box_occluders::diff(&vanilla.box_occluders, &modded.box_occluders, &exact)
      .unwrap()
      .len(),
    4
  );
}
