//! Tests for applying [`YmapDiffFields`], mostly as round trips: applying the diff of a modded
//! YMAP to its vanilla YMAP must rebuild the modded one.

use struct_diff::ApplyError;

use super::*;
use crate::common::{rotation::Rotation, triangle::Triangle};
use crate::core::format::ymap::compare::*;
use crate::core::format::ymap::model::{
  BoundingBox, GrassInstance, GrassInstanceBatch, Vector3, Vector4, YmapBoxOccluder,
  YmapCarGenerator, YmapDistantLodLight, YmapLodLight, YmapOccludeModel, YmapTimeCycleModifier,
};

/// Applies the diff of `modded` to `vanilla`, after a trip through the `.diff.json` form.
fn apply(
  vanilla: &Ymap,
  modded: &Ymap,
) -> Ymap {
  let diff = diff(vanilla, modded);
  let diff: YmapDiffFields = serde_json::from_value(serde_json::to_value(&diff).unwrap()).unwrap();
  apply_diff(&diff, vanilla).unwrap()
}

/// `vanilla` with `diff` applied.
fn apply_diff(
  diff: &YmapDiffFields,
  vanilla: &Ymap,
) -> Result<Ymap, ApplyError> {
  let mut ymap = vanilla.clone();
  ymap.apply(diff)?;
  Ok(ymap)
}

/// Asserts that applying the diff of `modded` to `vanilla` rebuilds `modded` exactly.
fn assert_round_trip(
  vanilla: &Ymap,
  modded: &Ymap,
) {
  assert_ne!(diff(vanilla, modded), YmapDiffFields::default(), "the test changes nothing");
  assert_eq!(&apply(vanilla, modded), modded);
}

#[test]
fn an_empty_diff_leaves_the_ymap_unchanged() {
  let vanilla = sample();
  assert_eq!(apply_diff(&YmapDiffFields::default(), &vanilla).unwrap(), vanilla);
}

#[test]
fn metadata_changes_are_applied() {
  let vanilla = base();
  let mut modded = vanilla.clone();
  modded.name = "renamed".into();
  modded.parent = "new_parent".into();
  modded.flags ^= 2;
  modded.content_flags ^= 1;
  modded.streaming_extents_min.x -= 100.0;
  modded.entities_extents_max.z += 5.5;
  modded.block.owner = "someone".into();
  modded.block.version += 1;
  assert_round_trip(&vanilla, &modded);
}

#[test]
fn entities_are_modified_removed_and_added_in_place() {
  let vanilla = Ymap {
    entities: vec![entity(1), entity(2), entity(3), entity(4)],
    ..base()
  };
  let mut modded = Ymap {
    entities: vec![entity(5), entity(1), entity(3), entity(4), entity(6)],
    ..base()
  };
  let changed = &mut modded.entities[2];
  changed.position.y += 12.5;
  changed.rotation = Rotation {
    x: 0.0,
    y: 0.0,
    z: 0.6,
    w: 0.8,
  };
  changed.archetype_name = "replaced".into();
  changed.lod_dist = 250.0;
  changed.tint_value = 3;
  assert_round_trip(&vanilla, &modded);
}

#[test]
fn entities_sharing_a_guid_rebuild_in_order() {
  let vanilla = Ymap {
    entities: vec![entity(1), entity(1)],
    ..base()
  };
  let mut modded = Ymap {
    entities: vec![entity(1), entity(1), entity(1)],
    ..base()
  };
  modded.entities[1].scale_z = 3.0;
  assert_round_trip(&vanilla, &modded);
}

#[test]
fn changes_within_the_tolerances_are_not_applied() {
  let vanilla = Ymap {
    entities: vec![entity(1)],
    ..base()
  };
  let mut modded = vanilla.clone();
  modded.entities[0].position.x += 0.0005; // below tolerance.ymap
  modded.entities[0].lod_dist = 321.0;
  let applied = apply(&vanilla, &modded);
  assert_eq!(applied.entities[0].position, vanilla.entities[0].position);
  assert_eq!(applied.entities[0].lod_dist, 321.0);
  assert_eq!(diff(&applied, &modded), YmapDiffFields::default());
}

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

#[test]
fn box_occluders_are_applied() {
  let vanilla = Ymap {
    box_occluders: vec![occluder(0), occluder(100), occluder(200)],
    ..base()
  };
  let mut modded = Ymap {
    box_occluders: vec![occluder(0), occluder(100), occluder(300)],
    ..base()
  };
  modded.box_occluders[1].i_sin_z = 5;
  modded.box_occluders[1].i_width = 8;
  assert_round_trip(&vanilla, &modded);
}

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
fn occlude_models_drop_listed_triangles_and_append_added_ones() {
  let vanilla = Ymap {
    occlude_models: vec![
      model(0.0, vec![triangle(0.0), triangle(2.0), triangle(4.0)]),
      model(100.0, vec![]),
    ],
    ..base()
  };
  let mut modded = Ymap {
    occlude_models: vec![model(0.0, vec![triangle(0.0), triangle(4.0), triangle(8.0)])],
    ..base()
  };
  modded.occlude_models[0].bmax.z = 12.5;
  modded.occlude_models[0].flags = 1;
  assert_round_trip(&vanilla, &modded);
}

#[test]
fn added_triangles_keep_their_position_among_kept_ones() {
  let vanilla = Ymap {
    occlude_models: vec![model(0.0, vec![triangle(0.0), triangle(2.0)])],
    ..base()
  };
  let modded = Ymap {
    occlude_models: vec![model(0.0, vec![triangle(8.0), triangle(0.0), triangle(2.0)])],
    ..base()
  };
  assert_round_trip(&vanilla, &modded);
}

fn lod_light(falloff: f32) -> YmapLodLight {
  YmapLodLight {
    falloff,
    hash: "0".into(),
    ..YmapLodLight::default()
  }
}

#[test]
fn lod_lights_are_applied() {
  let vanilla = Ymap {
    lod_lights: vec![lod_light(1.0), lod_light(2.0), lod_light(3.0)],
    ..base()
  };
  let modded = Ymap {
    lod_lights: vec![lod_light(2.0), lod_light(3.0), lod_light(9.0)],
    ..base()
  };
  assert_round_trip(&vanilla, &modded);
}

fn distant_light(
  position: &str,
  rgbi: &str,
) -> YmapDistantLodLight {
  YmapDistantLodLight {
    position: position.into(),
    rgbi: rgbi.into(),
  }
}

#[test]
fn distant_lod_lights_are_applied() {
  let lights = |items, num_street_lights| Ymap {
    distant_lod_lights: YmapDistantLodLights {
      items,
      num_street_lights,
      category: 0,
    },
    ..base()
  };
  let vanilla = lights(
    vec![distant_light("1 2 3", "10"), distant_light("4 5 6", "20"), distant_light("7 8 9", "30")],
    2,
  );
  let mut modded = lights(
    vec![
      distant_light("1 2 3", "10"),
      distant_light("4 5 6", "21"),
      distant_light("10 11 12", "40"),
    ],
    3,
  );
  modded.distant_lod_lights.category = 1;
  assert_round_trip(&vanilla, &modded);
}

#[test]
fn kept_items_stay_in_vanilla_order() {
  // Diffs do not record moves of kept items, so a reordering source rebuilds in vanilla order.
  let vanilla = Ymap {
    box_occluders: vec![occluder(0), occluder(100)],
    ..base()
  };
  let modded = Ymap {
    box_occluders: vec![occluder(100), occluder(0)],
    ..base()
  };
  assert_eq!(diff(&vanilla, &modded), YmapDiffFields::default());
  assert_eq!(apply(&vanilla, &modded), vanilla);
}

#[test]
fn distant_lod_light_positions_are_rewritten_with_their_changed_axes() {
  let mut light = distant_light("1 2 3", "10");
  let changes = YmapDistantLodLightFieldDiff {
    position: Some(PositionDiff {
      y: Some(2.25),
      ..Default::default()
    }),
    rgbi: None,
  };
  light.apply(&changes).unwrap();
  assert_eq!(light, distant_light("1 2.25 3", "10"));
}

#[test]
fn physics_dictionaries_are_added_but_never_removed() {
  let vanilla = Ymap {
    physics_dictionaries: vec!["a".into(), "b".into()],
    ..base()
  };
  let modded = Ymap {
    physics_dictionaries: vec!["b".into(), "c".into()],
    ..base()
  };
  assert_eq!(apply(&vanilla, &modded).physics_dictionaries, ["a", "b", "c"]);
}

fn modifier(x: f32) -> YmapTimeCycleModifier {
  YmapTimeCycleModifier {
    name: "tcm".into(),
    min_extents: at(x, 0.0, 0.0),
    max_extents: at(x + 10.0, 10.0, 10.0),
    percentage: 50.0,
    range: 20.0,
    start_hour: 0,
    end_hour: 23,
  }
}

#[test]
fn time_cycle_modifiers_are_applied() {
  let vanilla = Ymap {
    time_cycle_modifiers: vec![modifier(0.0), modifier(100.0), modifier(200.0)],
    ..base()
  };
  let mut modded = Ymap {
    time_cycle_modifiers: vec![modifier(0.0), modifier(100.0), modifier(300.0)],
    ..base()
  };
  modded.time_cycle_modifiers[1].max_extents.z = 30.5;
  modded.time_cycle_modifiers[1].name = "renamed".into();
  modded.time_cycle_modifiers[1].end_hour = 6;
  assert_round_trip(&vanilla, &modded);
}

fn generator(x: f32) -> YmapCarGenerator {
  YmapCarGenerator {
    position: at(x, 0.0, 0.0),
    orient_x: 1.0,
    orient_y: 0.0,
    perpendicular_length: 2.0,
    car_model: String::new(),
    flags: 0,
    body_color_remap_1: -1,
    body_color_remap_2: -1,
    body_color_remap_3: -1,
    body_color_remap_4: -1,
    pop_group: String::new(),
    livery: -1,
  }
}

#[test]
fn car_generators_are_applied() {
  let vanilla = Ymap {
    car_generators: vec![generator(0.0), generator(10.0), generator(20.0)],
    ..base()
  };
  let mut modded = Ymap {
    car_generators: vec![generator(30.0), generator(0.0), generator(20.0)],
    ..base()
  };
  modded.car_generators[2].car_model = "adder".into();
  modded.car_generators[2].orient_y = 0.5;
  modded.car_generators[2].livery = 2;
  assert_round_trip(&vanilla, &modded);
}

/// An instance at world `x` (y = z = 0) packed into a batch spanning `min_x..max_x`.
fn grass_instance(
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

fn grass_batch(
  min_x: f32,
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
      min: corner(min_x, 0.0),
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
    instances: xs.iter().map(|x| grass_instance(*x, min_x, max_x)).collect(),
  }
}

fn with_instanced_data(
  imap_link: &str,
  prop_instance_list: &[&str],
  grass_instance_list: Vec<GrassInstanceBatch>,
) -> Ymap {
  Ymap {
    instanced_data: YmapInstancedData {
      imap_link: imap_link.into(),
      prop_instance_list: prop_instance_list.iter().map(|name| (*name).into()).collect(),
      grass_instance_list,
    },
    ..base()
  }
}

#[test]
fn instanced_data_is_applied() {
  let vanilla = with_instanced_data(
    "link",
    &["prop_a", "prop_b"],
    vec![grass_batch(0.0, 10.0, &[1.0, 2.0, 3.0]), grass_batch(50.0, 60.0, &[55.0])],
  );
  let mut modded = with_instanced_data(
    "other_link",
    &["prop_b", "prop_c"],
    vec![grass_batch(0.0, 10.0, &[1.0, 3.0, 7.0]), grass_batch(80.0, 90.0, &[85.0])],
  );
  modded.instanced_data.grass_instance_list[0].instances[1].scale = 9;
  modded.instanced_data.grass_instance_list[0].instances[1].color = vec![0, 128, 0];
  assert_round_trip(&vanilla, &modded);
}

#[test]
fn grass_instances_keep_their_world_position_when_bounds_change() {
  let vanilla = with_instanced_data("", &[], vec![grass_batch(0.0, 10.0, &[1.0, 2.0, 3.0])]);
  let modded = with_instanced_data("", &[], vec![grass_batch(0.0, 20.0, &[1.0, 2.0, 3.0, 15.0])]);
  let applied = apply(&vanilla, &modded);
  assert_eq!(diff(&applied, &modded), YmapDiffFields::default());
  let batch = &applied.instanced_data.grass_instance_list[0];
  assert_eq!(batch.batch_aabb, modded.instanced_data.grass_instance_list[0].batch_aabb);
  let xs = batch
    .instances
    .iter()
    .map(|instance| {
      crate::core::format::ymap::compare::instanced_data::grass_world_position(batch, instance)
        .unwrap()
        .x
    })
    .collect::<Vec<_>>();
  for (x, expected) in xs.iter().zip([1.0, 2.0, 3.0, 15.0]) {
    assert!((x - expected).abs() < 0.001, "{xs:?}");
  }
}

#[test]
fn unchanged_items_fill_the_slots_left_in_vanilla_order() {
  let vanilla = Ymap {
    box_occluders: vec![occluder(0), occluder(100), occluder(200), occluder(300)],
    ..base()
  };
  let modded = Ymap {
    box_occluders: vec![occluder(500), occluder(0), occluder(200), occluder(600), occluder(300)],
    ..base()
  };
  assert_round_trip(&vanilla, &modded);
}

#[test]
fn every_kind_of_change_applies_together() {
  let vanilla = sample();
  let mut modded = vanilla.clone();
  modded.name = "renamed".into();
  modded.entities.swap_remove(0);
  modded.entities[0].position.z += 3.0;
  modded.entities.push(entity(0xDEAD_BEEF));
  modded.box_occluders.push(occluder(1000));
  modded.occlude_models.push(model(1000.0, vec![triangle(0.0)]));
  modded.lod_lights.push(lod_light(42.0));
  modded.physics_dictionaries.push("added_dictionary".into());
  modded.time_cycle_modifiers.push(modifier(1000.0));
  modded.car_generators.push(generator(1000.0));
  modded.instanced_data.grass_instance_list.push(grass_batch(1000.0, 1010.0, &[1005.0]));
  let applied = apply(&vanilla, &modded);
  assert_eq!(diff(&applied, &modded), YmapDiffFields::default());
}

#[test]
fn out_of_range_indices_are_rejected() {
  let vanilla = Ymap {
    box_occluders: vec![occluder(0)],
    ..base()
  };
  let removed_twice = YmapDiffFields {
    box_occluders: Some(vec![
      YmapBoxOccluderDiff::Removed {
        index: 0,
        implied: false,
      },
      YmapBoxOccluderDiff::Removed {
        index: 0,
        implied: false,
      },
    ]),
    ..Default::default()
  };
  let error = apply_diff(&removed_twice, &vanilla).unwrap_err().to_string();
  assert_eq!(error, "box_occluders: index 0 is changed twice");

  let missing_vanilla = YmapDiffFields {
    box_occluders: Some(vec![YmapBoxOccluderDiff::Removed {
      index: 1,
      implied: false,
    }]),
    ..Default::default()
  };
  let error = apply_diff(&missing_vanilla, &vanilla).unwrap_err().to_string();
  assert_eq!(error, "box_occluders: index 1 is out of 1 items");

  let past_the_end = YmapDiffFields {
    box_occluders: Some(vec![YmapBoxOccluderDiff::Added {
      after: Some(1),
      item: occluder(100),
    }]),
    ..Default::default()
  };
  let error = apply_diff(&past_the_end, &vanilla).unwrap_err().to_string();
  assert_eq!(error, "box_occluders: index 1 is out of 1 items");
}

#[test]
fn additions_follow_their_vanilla_item_even_when_it_is_removed() {
  let vanilla = Ymap {
    box_occluders: vec![occluder(0), occluder(100), occluder(200)],
    ..base()
  };
  let added = |after, x| YmapBoxOccluderDiff::Added {
    after,
    item: occluder(x),
  };
  let diff = YmapDiffFields {
    box_occluders: Some(vec![
      YmapBoxOccluderDiff::Removed {
        index: 1,
        implied: false,
      },
      added(Some(1), 150),
      added(None, -100),
      added(Some(1), 160),
      added(Some(2), 300),
    ]),
    ..Default::default()
  };
  assert_eq!(
    apply_diff(&diff, &vanilla).unwrap().box_occluders,
    [-100, 0, 150, 160, 200, 300].map(occluder)
  );
}
