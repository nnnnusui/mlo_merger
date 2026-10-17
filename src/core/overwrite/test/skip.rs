use super::*;
use serde_json::json;

fn diff() -> YmapDiffFields {
  serde_json::from_value(json!({
    "content_flags": 65,
    "streaming_extents_min": {"x": -452.29492, "z": 106.28345},
    "entities": [
      {"Modified": {"guid": 1, "before_index": 0, "after_index": 0,
        "changes": {"position": {"z": -961.18286}, "flags": 8}}},
      {"Removed": {"index": 1, "guid": 2}}
    ],
    "box_occluders": [{"Removed": {"index": 236}}, {"Removed": {"index": 237}}],
    "occlude_models": [
      {"Modified": {"before_index": 2, "after_index": 2, "changes": {
        "bmin": {"z": 1.2192383},
        "triangles": [{"Removed": {"index": 10}}, {"Removed": {"index": 11}}]
      }}}
    ]
  }))
  .unwrap()
}

#[test]
fn skips_fields_items_and_nested_items_selected_by_content() {
  let mut diff = diff();
  let unmatched = skip_diffs(
    &mut diff,
    &json!({
      "content_flags": true,
      "streaming_extents_min": {"x": true},
      "entities": [
        {"Modified": {"guid": 1, "changes": {"position": true}}},
        {"Removed": {"guid": 2}}
      ],
      "box_occluders": [{"Removed": {"index": 236}}],
      "occlude_models": [
        {"Modified": {"before_index": 2, "changes": {"triangles": [{"Removed": {"index": 10}}]}}}
      ]
    }),
  )
  .unwrap();
  assert!(unmatched.is_empty(), "{unmatched:?}");
  // Through text, as documents spell single-precision values.
  assert_eq!(
    serde_json::from_str::<serde_json::Value>(&serde_json::to_string(&diff).unwrap()).unwrap(),
    json!({
      "streaming_extents_min": {"z": 106.28345},
      "entities": [
        {"Modified": {"guid": 1, "before_index": 0, "after_index": 0, "changes": {"flags": 8}}}
      ],
      "box_occluders": [{"Removed": {"index": 237}}],
      "occlude_models": [
        {"Modified": {"before_index": 2, "after_index": 2, "changes": {
          "bmin": {"z": 1.2192383},
          "triangles": [{"Removed": {"index": 11}}]
        }}}
      ]
    })
  );
}

#[test]
fn drops_items_and_fields_left_empty_and_reports_unmatched_selectors() {
  let mut diff = diff();
  let unmatched = skip_diffs(
    &mut diff,
    &json!({
      "entities": [
        {"Modified": {"guid": 1, "changes": {"position": true, "flags": true}}},
        {"Removed": {"guid": 2}}
      ],
      "box_occluders": [{"Removed": {"index": 1}}],
      "streaming_extents_min": {"x": true, "z": true},
      "car_generators": true,
      "occlude_models": [{"Modified": {"before_index": 2, "changes": {"bmax": true}}}]
    }),
  )
  .unwrap();
  assert_eq!(
    unmatched,
    ["box_occluders[0]", "car_generators", "occlude_models[0].Modified.changes.bmax"]
  );
  assert!(diff.entities.is_none());
  assert!(diff.streaming_extents_min.is_none());
  assert_eq!(diff.box_occluders.unwrap().len(), 2);
}

#[test]
fn matches_single_precision_values_as_written_in_documents() {
  let mut diff = diff();
  let mut entities = serde_json::to_value(&diff.entities).unwrap();
  let mut entity =
    crate::core::format::gamefile::test_support::sample_ymap("parent_refs/vanilla_parent.ymap")
      .entities
      .remove(0);
  entity.position.x = -260.3641;
  entities
    .as_array_mut()
    .unwrap()
    .push(json!({"Added": {"after": 1, "entity": entity, "parent": "None"}}));
  diff.entities = serde_json::from_value(entities).unwrap();
  let unmatched = skip_diffs(
    &mut diff,
    &json!({"entities": [{"Added": {"entity": {"position": {"x": -260.3641}}}}]}),
  )
  .unwrap();
  assert!(unmatched.is_empty(), "{unmatched:?}");
  assert_eq!(diff.entities.unwrap().len(), 2);
}

#[test]
fn applies_only_against_the_files_it_was_written_for() {
  let fingerprint = |sha256: &str| OverwriteFingerprint {
    sha256: sha256.into(),
    size: 1,
  };
  let overwrite = StreamFileOverwrite {
    source: fingerprint("source"),
    vanilla: fingerprint("vanilla"),
    skip: json!({"content_flags": true}),
  };
  let mut diff = diff();
  overwrite.apply(&mut diff, &fingerprint("other"), &fingerprint("vanilla"), "map").unwrap();
  overwrite.apply(&mut diff, &fingerprint("source"), &fingerprint("other"), "map").unwrap();
  assert_eq!(diff.content_flags, Some(65));
  overwrite.apply(&mut diff, &fingerprint("source"), &fingerprint("vanilla"), "map").unwrap();
  assert_eq!(diff.content_flags, None);
  let invalid = StreamFileOverwrite {
    skip: json!({"flags": false}),
    ..overwrite
  };
  let mut diff = self::diff();
  diff.flags = Some(1);
  assert!(
    invalid.apply(&mut diff, &fingerprint("source"), &fingerprint("vanilla"), "map").is_err()
  );
}

#[test]
fn loads_resource_files_keyed_case_insensitively() {
  let root = std::env::temp_dir().join(format!("mlo_overwrite_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  assert!(Overwrites::load(&root).unwrap().keys().next().is_none());
  fs::create_dir_all(root.join("resources")).unwrap();
  let file = json!({
    "source": {"sha256": "a", "size": 1},
    "vanilla": {"sha256": "b", "size": 2},
    "skip": {"content_flags": true}
  });
  fs::write(
    root.join("resources/Cfx-Map.json"),
    serde_json::to_vec(&json!({"schema_version": 1, "files": {"Vanilla\\Map.ymap": file}}))
      .unwrap(),
  )
  .unwrap();
  fs::write(root.join("resources/notes.txt"), "ignored").unwrap();
  let overwrites = Overwrites::load(&root).unwrap();
  assert_eq!(overwrites.keys().collect::<Vec<_>>(), [("cfx-map", "vanilla/map.ymap")]);
  let overwrite = overwrites.stream_file("cfx-map", "vanilla/MAP.ymap").unwrap();
  assert_eq!(overwrite.vanilla.size, 2);
  fs::write(
    root.join("resources/cfx-map.json"),
    serde_json::to_vec(&json!({"schema_version": 2, "files": {}})).unwrap(),
  )
  .unwrap();
  let _ = fs::remove_file(root.join("resources/Cfx-Map.json"));
  assert!(Overwrites::load(&root).is_err());
  fs::remove_dir_all(root).unwrap();
}
