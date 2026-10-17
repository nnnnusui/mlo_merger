use serde_json::json;

use super::*;
use crate::core::format::ymap::compare::*;

fn with_entities(entities: Vec<YmapEntity>) -> Ymap {
  Ymap {
    entities,
    ..base()
  }
}

#[test]
fn modified_entities_list_changed_fields_beyond_the_tolerance() {
  let vanilla = with_entities(vec![entity(1)]);
  let mut modded = vanilla.clone();
  let moved = &mut modded.entities[0];
  moved.position.x += 0.0005; // below tolerance.ymap
  moved.position.y = 12.5;
  moved.lod_dist = 250.0;
  moved.archetype_name = "replaced".into();
  assert_eq!(
    json(&vanilla, &modded),
    json!({
      "entities": [{
        "Modified": {
          "guid": 1,
          "before_index": 0,
          "after_index": 0,
          "changes": {
            "archetype_name": "replaced",
            "position": { "y": 12.5 },
            "lod_dist": 250.0,
          },
        },
      }],
    })
  );
}

#[test]
fn removed_entities_keep_only_their_index_and_guid() {
  let vanilla = with_entities(vec![entity(1), entity(2)]);
  let modded = with_entities(vec![entity(1)]);
  assert_eq!(
    json(&vanilla, &modded),
    json!({ "entities": [{ "Removed": { "index": 1, "guid": 2 } }] })
  );
}

#[test]
fn added_entities_hold_the_whole_entity() {
  let vanilla = with_entities(vec![entity(1)]);
  let modded = with_entities(vec![entity(1), entity(3)]);
  let diffs = json(&vanilla, &modded);
  assert_eq!(diffs["entities"][0]["Added"]["after"], json!(0));
  assert_eq!(diffs["entities"][0]["Added"]["entity"], serde_json::to_value(entity(3)).unwrap());
}

#[test]
fn shared_guids_pair_in_order_and_report_the_rest() {
  let vanilla = with_entities(vec![entity(1), entity(1)]);
  let mut modded = with_entities(vec![entity(1), entity(1), entity(1)]);
  modded.entities[1].scale_z = 3.0;
  let diffs = json(&vanilla, &modded);
  assert_eq!(diffs["entities"][0]["Modified"]["before_index"], json!(1));
  assert_eq!(diffs["entities"][0]["Modified"]["changes"], json!({ "scale_z": 3.0 }));
  assert_eq!(diffs["entities"][1]["Added"]["after"], json!(1));
  assert_eq!(diffs["entities"].as_array().unwrap().len(), 2);
}

#[test]
fn the_configured_tolerance_widens_float_comparisons() {
  let vanilla = with_entities(vec![entity(1)]);
  let mut modded = vanilla.clone();
  modded.entities[0].position.z += 0.5;
  let loose = MatchTolerances {
    ymap: 1.0,
    ..Default::default()
  };
  assert!(diff_with(&vanilla, &modded, &loose).entities.is_none());
  assert_eq!(diff(&vanilla, &modded).entities.unwrap_or_default().len(), 1);
}

// Moved from `ymap_entity_diff.rs`: the diff function on its own.

fn three_entities() -> Ymap {
  let xml: XmlYmap = quick_xml::de::from_str(include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/sample/parent_refs/vanilla_parent.ymap.xml"
  )))
  .unwrap();
  let mut ymap: Ymap = xml.into();
  let entity = ymap.entities[0].clone();
  ymap.entities = (0..3)
    .map(|guid| YmapEntity {
      guid: guid + 1,
      ..entity.clone()
    })
    .collect();
  ymap
}

fn entity_diff(
  vanilla: &Ymap,
  modded: &Ymap,
) -> Vec<YmapEntityDiff> {
  let ctx = YmapDiffContext::new(MatchTolerances::default(), (vanilla, None), (modded, None));
  compare::entities::diff(&vanilla.entities, &modded.entities, &ctx).unwrap_or_default()
}

#[test]
fn ignores_float_changes_below_tolerance() {
  let vanilla = three_entities();
  let mut modded = vanilla.clone();
  modded.entities[0].position.x += 0.0005;
  modded.entities[1].rotation.w += 0.0005;
  modded.entities[2].lod_dist += 0.0005;
  assert!(entity_diff(&vanilla, &modded).is_empty());
}

#[test]
fn reports_modified_removed_and_added_entities() {
  let vanilla = three_entities();
  let mut modded = vanilla.clone();
  modded.entities[0].position.x += 0.0005;
  modded.entities[0].position.y += 1.0;
  modded.entities[1].archetype_name = "replaced".into();
  modded.entities.remove(2);
  modded.entities.push(YmapEntity {
    guid: 99,
    ..vanilla.entities[0].clone()
  });

  let diffs = entity_diff(&vanilla, &modded);
  assert_eq!(diffs.len(), 4);
  let YmapEntityDiff::Modified {
    guid: 1,
    before_index: 0,
    after_index: 0,
    changes,
    parent: None,
  } = &diffs[0]
  else {
    panic!("{:?}", diffs[0]);
  };
  let y = vanilla.entities[0].position.y + 1.0;
  assert_eq!(
    changes,
    &YmapEntityFieldDiff {
      position: Some(PositionDiff {
        y: Some(y),
        ..PositionDiff::default()
      }),
      ..YmapEntityFieldDiff::default()
    }
  );
  assert!(matches!(
    &diffs[1],
    YmapEntityDiff::Modified { guid: 2, changes, .. }
      if changes.archetype_name.as_deref() == Some("replaced")
        && changes == &YmapEntityFieldDiff { archetype_name: changes.archetype_name.clone(), ..Default::default() }
  ));
  assert!(matches!(
    &diffs[2],
    YmapEntityDiff::Removed {
      index: 2,
      guid: 3
    }
  ));
  assert!(
    matches!(&diffs[3], YmapEntityDiff::Added { after: Some(1), entity, .. } if entity.guid == 99)
  );
}

#[test]
fn pairs_shared_guids_in_order_and_reports_the_rest() {
  let mut vanilla = three_entities();
  vanilla.entities[1].guid = 1;
  let mut modded = vanilla.clone();
  modded.entities[1].scale_z += 1.0;
  modded.entities.insert(2, modded.entities[0].clone());

  let diffs = entity_diff(&vanilla, &modded);
  assert_eq!(diffs.len(), 2, "{diffs:?}");
  assert!(matches!(
    &diffs[0],
    YmapEntityDiff::Modified {
      guid: 1,
      before_index: 1,
      after_index: 1,
      ..
    }
  ));
  assert!(
    matches!(&diffs[1], YmapEntityDiff::Added { after: Some(1), entity, .. } if entity.guid == 1)
  );
}

/// An entity at `lod_level` pointing at `parent_index`, of its own YMAP unless `in_parent`.
fn linked(
  guid: u32,
  lod_level: &str,
  parent_index: i32,
  in_parent: bool,
) -> YmapEntity {
  let mut entity = entity(guid);
  entity.lod_level = lod_level.into();
  entity.parent_index = parent_index;
  entity.flags = if in_parent { entity.flags | 8 } else { entity.flags & !8 };
  entity
}

#[test]
fn parents_compare_by_the_entity_they_point_at() {
  let lod = "LODTYPES_DEPTH_LOD";
  let hd = "LODTYPES_DEPTH_HD";
  let vanilla = with_entities(vec![linked(1, lod, -1, false), linked(2, hd, 0, false)]);
  // An entity inserted first shifts the parent, so the child's index changes but not its parent.
  let shifted = with_entities(vec![
    linked(3, lod, -1, false),
    linked(1, lod, -1, false),
    linked(2, hd, 1, false),
  ]);
  let ctx =
    |modded| YmapDiffContext::new(MatchTolerances::default(), (&vanilla, None), (modded, None));
  let diffs =
    compare::entities::diff(&vanilla.entities, &shifted.entities, &ctx(&shifted)).unwrap();
  assert!(
    matches!(
      diffs.as_slice(),
      [YmapEntityDiff::Added {
        after: None,
        ..
      }]
    ),
    "{diffs:?}"
  );
  // Pointing at the new entity instead is a parent change.
  let reparented = Ymap {
    entities: vec![linked(3, lod, -1, false), linked(1, lod, -1, false), linked(2, hd, 0, false)],
    ..base()
  };
  let diffs =
    compare::entities::diff(&vanilla.entities, &reparented.entities, &ctx(&reparented)).unwrap();
  assert!(
    diffs.iter().any(|diff| matches!(
      diff,
      YmapEntityDiff::Modified {
        guid: 2,
        parent: Some(EntityParent::Own {
          guid: 3
        }),
        ..
      }
    )),
    "{diffs:?}"
  );
}

#[test]
fn external_parents_are_named_by_their_map_and_guid() {
  let parent_entities = vec![entity(10), entity(20)];
  let mut child = with_entities(vec![linked(1, "LODTYPES_DEPTH_HD", 1, true)]);
  child.parent = "parent_map".into();
  assert_eq!(
    entity_parents(&child, Some(&parent_entities)),
    vec![EntityParent::Parent {
      map: crate::core::format::ymap::reference_hash("parent_map"),
      guid: 20,
    }]
  );
  assert_eq!(
    entity_parents(&child, None),
    vec![EntityParent::Unresolved {
      in_parent: true,
      index: 1,
    }]
  );
}
