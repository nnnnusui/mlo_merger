use serde_json::json;

use super::*;
use crate::core::format::ymap::compare::*;
use crate::{common::triangle::Triangle, core::format::ymap::model::YmapOccludeModel};

fn triangle(offset: f32) -> Triangle {
  Triangle {
    corner_1: at(offset, 0.0, 0.0),
    corner_2: at(offset + 1.0, 0.0, 0.0),
    corner_3: at(offset, 1.0, 0.0),
  }
}

fn model(
  x: f32,
  triangles: Vec<Triangle>,
) -> YmapOccludeModel {
  YmapOccludeModel {
    bmin: at(x, 0.0, 0.0),
    bmax: at(x + 10.0, 10.0, 10.0),
    triangles,
    flags: 0,
  }
}

#[test]
fn occlude_models_list_triangle_changes_by_vanilla_index() {
  let vanilla = Ymap {
    occlude_models: vec![model(0.0, vec![triangle(0.0), triangle(2.0)]), model(100.0, vec![])],
    ..base()
  };
  let mut modded = Ymap {
    occlude_models: vec![model(0.0, vec![triangle(0.0), triangle(4.0)])],
    ..base()
  };
  modded.occlude_models[0].bmax.z = 12.5;
  modded.occlude_models[0].flags = 1;
  assert_eq!(
    json(&vanilla, &modded),
    json!({
      "occlude_models": [
        {
          "Modified": {
            "vanilla_index": 0,
            "modded_index": 0,
            "changes": {
              "bmax": { "z": 12.5 },
              "flags": 1,
              "removed_triangles": [1],
              "added_triangles": [serde_json::to_value(triangle(4.0)).unwrap()],
            },
          },
        },
        { "Removed": { "index": 1 } },
      ],
    })
  );
}

#[test]
fn occlude_models_pair_within_the_xy_tolerance() {
  let vanilla = Ymap {
    occlude_models: vec![model(0.0, vec![])],
    ..base()
  };
  let mut modded = vanilla.clone();
  modded.occlude_models[0].bmin.x = 0.005; // within tolerance.ymap_occlude_model
  modded.occlude_models[0].bmax.x = 10.005;
  let diffs = json(&vanilla, &modded);
  assert!(diffs["occlude_models"][0]["Modified"].is_object(), "{diffs}");
}

// Moved from `ymap_occlude_model_diff.rs`: the diff function on its own.

#[test]
fn lists_removed_triangle_indices_and_added_triangles() {
  let vanilla = Ymap {
    occlude_models: vec![model(0.0, vec![triangle(0.0), triangle(2.0), triangle(0.0)])],
    ..base()
  };
  let mut modded = vanilla.clone();
  modded.occlude_models[0].triangles = vec![triangle(0.0), triangle(4.0)];
  modded.occlude_models[0].bmin.z = 0.5;
  modded.occlude_models[0].flags = 1;

  let diffs = occlude_model_diffs(&vanilla, &modded, &MatchTolerances::default());
  assert_eq!(
    diffs,
    vec![YmapOccludeModelDiff::Modified {
      vanilla_index: 0,
      modded_index: 0,
      changes: YmapOccludeModelFieldDiff {
        bmin: Some(PositionDiff {
          z: Some(0.5),
          ..Default::default()
        }),
        flags: Some(1),
        removed_triangles: vec![1, 2],
        added_triangles: vec![triangle(4.0)],
        ..Default::default()
      },
    }]
  );
}
