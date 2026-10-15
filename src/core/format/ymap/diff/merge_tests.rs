use super::{
  YmapBoxOccluderDiff, YmapCarGeneratorDiff, YmapDistantLodLightDiff, YmapLodLightDiff,
  YmapOccludeModelDiff, YmapOccludeModelTriangleDiff, YmapTimeCycleModifierDiff,
  resolve_box_occluder_removals, resolve_car_generator_removals,
  resolve_distant_lod_light_removals, resolve_lod_light_removals, resolve_occlude_model_removals,
  resolve_time_cycle_modifier_removals,
};
use crate::common::{position::Position, triangle::Triangle};
use crate::core::format::ymap::model::{
  YmapBoxOccluder, YmapCarGenerator, YmapDistantLodLight, YmapLodLight, YmapOccludeModel,
  YmapTimeCycleModifier, ymap_occlude_model::YmapOccludeModelStructDiffEnum,
  ymap_time_cycle_modifier::YmapTimeCycleModifierStructDiffEnum,
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

fn lod_light() -> YmapLodLight {
  YmapLodLight {
    direction: Position {
      x: 1.0,
      y: 2.0,
      z: 3.0,
    },
    ..Default::default()
  }
}

fn distant_lod_light() -> YmapDistantLodLight {
  YmapDistantLodLight {
    position: "1 2 3".into(),
    rgbi: "255".into(),
  }
}

fn car_generator() -> YmapCarGenerator {
  YmapCarGenerator {
    position: Position {
      x: 1.0,
      y: 2.0,
      z: 3.0,
    },
    orient_x: 1.0,
    orient_y: 0.0,
    perpendicular_length: 5.0,
    car_model: "adder".into(),
    flags: 0,
    body_color_remap_1: 0,
    body_color_remap_2: 0,
    body_color_remap_3: 0,
    body_color_remap_4: 0,
    pop_group: String::new(),
    livery: 0,
  }
}

fn time_cycle_modifier() -> YmapTimeCycleModifier {
  YmapTimeCycleModifier {
    name: "test".into(),
    min_extents: Position {
      x: 1.0,
      y: 2.0,
      z: 3.0,
    },
    max_extents: Position {
      x: 4.0,
      y: 5.0,
      z: 6.0,
    },
    percentage: 50.0,
    range: 5.0,
    start_hour: 0,
    end_hour: 23,
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

#[test]
fn lod_light_removal_wins_over_matching_addition_and_modification() {
  let item = lod_light();
  let mut diffs = vec![
    YmapLodLightDiff::Added(item.clone()),
    YmapLodLightDiff::Modified {
      vanilla: item.clone(),
      modded: item.clone(),
    },
    YmapLodLightDiff::Removed(item),
  ];
  resolve_lod_light_removals(&mut diffs);
  assert_eq!(diffs.len(), 1);
  assert!(matches!(diffs[0], YmapLodLightDiff::Removed(_)));
}

#[test]
fn distant_lod_light_removal_wins_over_matching_addition_and_modification() {
  let item = distant_lod_light();
  let mut diffs = vec![
    YmapDistantLodLightDiff::Added(item.clone()),
    YmapDistantLodLightDiff::Modified {
      vanilla: item.clone(),
      modded: item.clone(),
    },
    YmapDistantLodLightDiff::Removed(item),
  ];
  resolve_distant_lod_light_removals(&mut diffs);
  assert_eq!(diffs.len(), 1);
  assert!(matches!(diffs[0], YmapDistantLodLightDiff::Removed(_)));
}

#[test]
fn car_generator_removal_wins_over_matching_addition_and_modification() {
  let item = car_generator();
  let mut diffs = vec![
    YmapCarGeneratorDiff::Added(item.clone()),
    YmapCarGeneratorDiff::Modified {
      vanilla: item.clone(),
      modded: item.clone(),
    },
    YmapCarGeneratorDiff::Removed(item),
  ];
  resolve_car_generator_removals(&mut diffs);
  assert_eq!(diffs.len(), 1);
  assert!(matches!(diffs[0], YmapCarGeneratorDiff::Removed(_)));
}

#[test]
fn time_cycle_modifier_removal_wins_over_matching_addition_and_modification() {
  let item = time_cycle_modifier();
  let mut diffs = vec![
    YmapTimeCycleModifierDiff::Added(item.clone()),
    YmapTimeCycleModifierDiff::Modified {
      vanilla: item.clone(),
      diffs: Vec::<YmapTimeCycleModifierStructDiffEnum>::new(),
    },
    YmapTimeCycleModifierDiff::Removed(item),
  ];
  resolve_time_cycle_modifier_removals(&mut diffs);
  assert_eq!(diffs.len(), 1);
  assert!(matches!(diffs[0], YmapTimeCycleModifierDiff::Removed(_)));
}
