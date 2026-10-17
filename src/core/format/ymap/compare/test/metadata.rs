use serde_json::json;

use super::*;

#[test]
fn identical_ymaps_have_no_diffs() {
  let vanilla = base();
  assert_eq!(diff(&vanilla, &vanilla), YmapDiffFields::default());
  assert_eq!(json(&vanilla, &vanilla), json!({}));
}

#[test]
fn scalar_fields_hold_their_new_values() {
  let vanilla = base();
  let modded = Ymap {
    name: "renamed".into(),
    parent: "other_parent".into(),
    flags: vanilla.flags ^ 2,
    content_flags: vanilla.content_flags | 64,
    ..vanilla.clone()
  };
  assert_eq!(
    json(&vanilla, &modded),
    json!({
      "name": "renamed",
      "parent": "other_parent",
      "flags": vanilla.flags ^ 2,
      "content_flags": vanilla.content_flags | 64,
    })
  );
}

#[test]
fn extents_list_only_the_changed_axes_of_each_corner() {
  let vanilla = base();
  let mut modded = vanilla.clone();
  modded.streaming_extents_min.x = 100.5;
  modded.streaming_extents_max = at(1.5, 2.5, 3.5);
  modded.entities_extents_max.z = -4.25;
  let mut expected_max = json!({});
  for (axis, before, after) in [
    ("x", vanilla.streaming_extents_max.x, 1.5),
    ("y", vanilla.streaming_extents_max.y, 2.5),
    ("z", vanilla.streaming_extents_max.z, 3.5),
  ] {
    if before != after {
      expected_max[axis] = json!(after);
    }
  }
  assert_eq!(
    json(&vanilla, &modded),
    json!({
      "streaming_extents_min": { "x": 100.5 },
      "streaming_extents_max": expected_max,
      "entities_extents_max": { "z": -4.25 },
    })
  );
}

#[test]
fn block_lists_only_its_changed_fields() {
  let vanilla = base();
  let mut modded = vanilla.clone();
  modded.block.exported_by = "exporter".into();
  modded.block.time = "01 January 2026".into();
  assert_eq!(
    json(&vanilla, &modded),
    json!({ "block": { "exported_by": "exporter", "time": "01 January 2026" } })
  );
}
