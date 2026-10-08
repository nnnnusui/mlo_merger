use super::{
  YmapBoxOccluderDiff, YmapOccludeModelDiff, YmapOccludeModelTriangleDiff,
  resolve_box_occluder_removals, resolve_occlude_model_removals,
};
use crate::core::common::{position::Position, triangle::Triangle};
use crate::core::format::ymap::model::{
  YmapBoxOccluder, YmapOccludeModel, ymap_occlude_model::YmapOccludeModelStructDiffEnum,
};

fn box_occluder() -> YmapBoxOccluder {
  YmapBoxOccluder {
    i_center_x: 10,
    i_center_y: 20,
    i_center_z: 30,
    i_cos_z: 0,
    i_length: 4,
    i_width: 5,
    i_height: 6,
    i_sin_z: 0,
  }
}

fn occlude_model() -> YmapOccludeModel {
  YmapOccludeModel {
    bmin: Position {
      x: 0.0,
      y: 0.0,
      z: 0.0,
    },
    bmax: Position {
      x: 4.0,
      y: 4.0,
      z: 4.0,
    },
    triangles: Vec::new(),
    flags: 0,
  }
}

fn triangle(offset: f32) -> Triangle {
  Triangle {
    corner_1: Position {
      x: offset,
      y: 0.0,
      z: 0.0,
    },
    corner_2: Position {
      x: offset + 1.0,
      y: 0.0,
      z: 0.0,
    },
    corner_3: Position {
      x: offset,
      y: 1.0,
      z: 0.0,
    },
  }
}

#[test]
fn box_occluder_removal_wins_over_matching_addition_and_modification() {
  let item = box_occluder();
  let mut diffs = vec![
    YmapBoxOccluderDiff::Added(item.clone()),
    YmapBoxOccluderDiff::Modified {
      vanilla: item.clone(),
      diffs: Vec::new(),
    },
    YmapBoxOccluderDiff::Removed(item),
  ];

  resolve_box_occluder_removals(&mut diffs);

  assert_eq!(diffs.len(), 1);
  assert!(matches!(diffs[0], YmapBoxOccluderDiff::Removed(_)));
}

#[test]
fn occlude_model_removal_wins_over_matching_addition_and_modification() {
  let model = occlude_model();
  let mut diffs = vec![
    YmapOccludeModelDiff::Added(model.clone()),
    YmapOccludeModelDiff::Modified {
      vanilla: model.clone(),
      diffs: Vec::<YmapOccludeModelStructDiffEnum>::new(),
      triangle_diffs: Vec::new(),
    },
    YmapOccludeModelDiff::Removed(model),
  ];

  resolve_occlude_model_removals(&mut diffs);

  assert_eq!(diffs.len(), 1);
  assert!(matches!(diffs[0], YmapOccludeModelDiff::Removed(_)));
}

#[test]
fn removed_triangle_wins_over_matching_addition_in_either_merge_order() {
  let model = occlude_model();
  let removed = triangle(0.0);
  let other = triangle(4.0);
  let additions = || YmapOccludeModelDiff::Modified {
    vanilla: model.clone(),
    diffs: Vec::new(),
    triangle_diffs: vec![YmapOccludeModelTriangleDiff::Added(removed.clone())],
  };
  let removals = || YmapOccludeModelDiff::Modified {
    vanilla: model.clone(),
    diffs: Vec::new(),
    triangle_diffs: vec![
      YmapOccludeModelTriangleDiff::Removed(removed.clone()),
      YmapOccludeModelTriangleDiff::Added(other.clone()),
    ],
  };

  for mut diffs in [vec![additions(), removals()], vec![removals(), additions()]] {
    resolve_occlude_model_removals(&mut diffs);
    let triangle_diffs = diffs
      .iter()
      .flat_map(|diff| match diff {
        YmapOccludeModelDiff::Modified {
          triangle_diffs,
          ..
        } => triangle_diffs.iter().collect::<Vec<_>>(),
        _ => Vec::new(),
      })
      .collect::<Vec<_>>();
    assert_eq!(
      triangle_diffs
        .iter()
        .filter(|diff| matches!(diff, YmapOccludeModelTriangleDiff::Removed(_)))
        .count(),
      1
    );
    assert_eq!(
      triangle_diffs
        .iter()
        .filter(|diff| matches!(diff, YmapOccludeModelTriangleDiff::Added(_)))
        .count(),
      1
    );
    assert!(triangle_diffs.iter().any(|diff| matches!(
      diff,
      YmapOccludeModelTriangleDiff::Added(triangle) if triangle == &other
    )));
  }
}
