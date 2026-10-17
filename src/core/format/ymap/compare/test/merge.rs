//! Tests for [`merge_diffs`], combining the diffs of several sources from one vanilla YMAP.

use struct_diff::Diff;

use super::*;
use crate::core::{
  config::blacklist::{BlacklistConfig, OccludeModelBlacklist},
  format::ymap::{
    compare::merge_diffs,
    model::{YmapBoxOccluder, YmapOccludeModel},
  },
};

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

/// `vanilla` with the diffs of every source in `sources` merged in, in order.
fn merged(
  vanilla: &Ymap,
  sources: &[&Ymap],
  blacklist: Option<&BlacklistConfig>,
) -> Ymap {
  let diffs = sources.iter().map(|source| diff(vanilla, source)).collect::<Vec<_>>();
  let mut ymap = vanilla.clone();
  ymap.apply(&merge_diffs(vanilla, &diffs, blacklist)).unwrap();
  ymap
}

#[test]
fn a_single_source_is_rebuilt_in_place() {
  let vanilla = Ymap {
    entities: vec![entity(1), entity(2), entity(3)],
    ..base()
  };
  let mut modded = Ymap {
    entities: vec![entity(4), entity(1), entity(3)],
    ..base()
  };
  modded.entities[2].lod_dist = 999.0;
  let merged = merged(&vanilla, &[&modded], None);
  assert_eq!(merged.entities, modded.entities);
}

#[test]
fn removals_win_and_the_first_modification_wins() {
  let vanilla = Ymap {
    entities: vec![entity(1), entity(2)],
    ..base()
  };
  let mut first = vanilla.clone();
  first.entities[0].lod_dist = 150.0;
  first.entities[1].lod_dist = 150.0;
  let mut second = vanilla.clone();
  second.entities[0].lod_dist = 250.0;
  second.entities.remove(1);
  for sources in [[&first, &second], [&second, &first]] {
    let merged = merged(&vanilla, &sources, None);
    assert_eq!(merged.entities.len(), 1, "the removal wins in either order");
    let expected = if sources[0] == &first { 150.0 } else { 250.0 };
    assert_eq!(merged.entities[0].lod_dist, expected);
  }
}

#[test]
fn additions_of_every_source_keep_their_place() {
  let vanilla = Ymap {
    box_occluders: vec![occluder(0), occluder(100)],
    ..base()
  };
  let mut first = vanilla.clone();
  first.box_occluders.insert(0, occluder(300));
  let mut second = vanilla.clone();
  second.box_occluders.remove(0);
  second.box_occluders.push(occluder(400));
  let merged = merged(&vanilla, &[&first, &second], None);
  assert_eq!(merged.box_occluders, vec![occluder(300), occluder(100), occluder(400)]);
}

#[test]
fn an_item_replaced_by_one_source_keeps_its_index_beside_removals_of_another() {
  // Like a light one source moves and another source's removal elsewhere, where the index must
  // stay aligned with a parallel array.
  let vanilla = Ymap {
    box_occluders: [0, 100, 200, 300].map(occluder).to_vec(),
    ..base()
  };
  let mut first = vanilla.clone();
  first.box_occluders[1] = occluder(150);
  let mut second = vanilla.clone();
  second.box_occluders.remove(3);
  let merged = merged(&vanilla, &[&first, &second], None);
  assert_eq!(merged.box_occluders, [0, 150, 200].map(occluder));
}

#[test]
fn blacklisted_occlude_models_are_not_added() {
  let model = |x: f32| YmapOccludeModel {
    bmin: at(x, 0.0, 0.0),
    bmax: at(x + 10.0, 10.0, 10.0),
    triangles: Vec::new(),
    flags: 0,
  };
  let vanilla = base();
  let modded = Ymap {
    occlude_models: vec![model(0.0), model(100.0)],
    ..base()
  };
  let blacklist = BlacklistConfig {
    occlude_models: vec![OccludeModelBlacklist {
      bmin: Some([100.0, 0.0, 0.0]),
      bmax: None,
    }],
  };
  assert_eq!(merged(&vanilla, &[&modded], Some(&blacklist)).occlude_models, vec![model(0.0)]);
}

#[test]
fn references_skip_names_already_present_and_keep_the_first_parent() {
  let vanilla = Ymap {
    physics_dictionaries: vec!["kept".into()],
    ..base()
  };
  let mut first = vanilla.clone();
  first.parent = "first_parent".into();
  first.physics_dictionaries = vec!["kept".into(), "added".into()];
  first.instanced_data.prop_instance_list = vec!["prop".into()];
  let mut second = vanilla.clone();
  second.parent = "second_parent".into();
  second.physics_dictionaries = vec!["Added".into(), "other".into()];
  second.instanced_data.prop_instance_list = vec!["PROP".into()];
  let merged = merged(&vanilla, &[&first, &second], None);
  assert_eq!(merged.parent, "first_parent");
  assert_eq!(merged.physics_dictionaries, ["kept", "added", "other"]);
  assert_eq!(merged.instanced_data.prop_instance_list, ["prop"]);
}

#[test]
fn content_flags_are_combined_and_other_metadata_stays_vanilla() {
  let vanilla = Ymap {
    content_flags: 1,
    ..base()
  };
  let mut first = vanilla.clone();
  first.content_flags = 2;
  first.name = "renamed".into();
  first.streaming_extents_max.x += 50.0;
  first.block.owner = "someone".into();
  let mut second = vanilla.clone();
  second.content_flags = 4;
  let merged = merged(&vanilla, &[&first, &second], None);
  assert_eq!(merged.content_flags, 7);
  assert_eq!(
    Ymap {
      content_flags: vanilla.content_flags,
      ..merged
    },
    vanilla
  );
}

#[test]
fn occlude_models_keep_the_cut_outs_and_fields_of_every_source() {
  let triangle = |offset: f32| crate::common::triangle::Triangle {
    corner_1: at(offset, 0.0, 0.0),
    corner_2: at(offset + 1.0, 0.0, 0.0),
    corner_3: at(offset, 1.0, 0.0),
  };
  let model = YmapOccludeModel {
    bmin: at(0.0, 0.0, 0.0),
    bmax: at(10.0, 10.0, 10.0),
    triangles: (0..4).map(|offset| triangle(offset as f32)).collect(),
    flags: 0,
  };
  let vanilla = Ymap {
    occlude_models: vec![model.clone()],
    ..base()
  };
  let mut first = vanilla.clone();
  first.occlude_models[0].triangles.remove(1);
  first.occlude_models[0].bmin.z = -5.0;
  let mut second = vanilla.clone();
  second.occlude_models[0].triangles.remove(3);
  second.occlude_models[0].triangles.push(triangle(9.0));
  second.occlude_models[0].bmin.z = -9.0;
  second.occlude_models[0].bmax.x = 20.0;
  let merged = merged(&vanilla, &[&first, &second], None);
  assert_eq!(
    merged.occlude_models,
    vec![YmapOccludeModel {
      bmin: at(0.0, 0.0, -5.0),
      bmax: at(20.0, 10.0, 10.0),
      triangles: vec![triangle(0.0), triangle(2.0), triangle(9.0)],
      flags: 0,
    }]
  );
}

#[test]
fn a_removal_by_another_source_drops_a_replacement() {
  // One source moves the second occluder out of pairing range, another removes it.
  let vanilla = Ymap {
    box_occluders: [0, 100, 200].map(occluder).to_vec(),
    ..base()
  };
  let mut moved = vanilla.clone();
  moved.box_occluders[1] = occluder(150);
  let mut removed = vanilla.clone();
  removed.box_occluders.remove(1);
  for sources in [[&moved, &removed], [&removed, &moved]] {
    assert_eq!(merged(&vanilla, &sources, None).box_occluders, [0, 200].map(occluder));
  }
}

#[test]
fn a_blacklisted_replacement_removes_the_vanilla_model() {
  let model = |x: f32| YmapOccludeModel {
    bmin: at(x, 0.0, 0.0),
    bmax: at(x + 10.0, 10.0, 10.0),
    triangles: Vec::new(),
    flags: 0,
  };
  let vanilla = Ymap {
    occlude_models: vec![model(0.0), model(100.0)],
    ..base()
  };
  let modded = Ymap {
    occlude_models: vec![model(0.0), model(300.0)],
    ..base()
  };
  let blacklist = BlacklistConfig {
    occlude_models: vec![OccludeModelBlacklist {
      bmin: Some([300.0, 0.0, 0.0]),
      bmax: None,
    }],
  };
  assert_eq!(merged(&vanilla, &[&modded], None).occlude_models, vec![model(0.0), model(300.0)]);
  assert_eq!(merged(&vanilla, &[&modded], Some(&blacklist)).occlude_models, vec![model(0.0)]);
}
