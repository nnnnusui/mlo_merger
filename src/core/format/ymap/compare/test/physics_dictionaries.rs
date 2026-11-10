use serde_json::json;

use super::*;

fn with_dictionaries(names: &[&str]) -> Ymap {
  Ymap {
    physics_dictionaries: names.iter().map(|name| name.to_string()).collect(),
    ..base()
  }
}

#[test]
fn physics_dictionaries_compare_by_name() {
  let vanilla = with_dictionaries(&["a", "b"]);
  assert_eq!(
    json(&vanilla, &with_dictionaries(&["b", "c"])),
    json!({
      "physics_dictionaries": [
        { "Removed": { "base_index": 0 } },
        { "Added": { "anchor_base_index": 1, "target_index": 1, "item": "c" } },
      ],
    })
  );
  assert_eq!(
    json(&vanilla, &with_dictionaries(&["b"])),
    json!({ "physics_dictionaries": [{ "Removed": { "base_index": 0 } }] })
  );
  // Names match by hash.
  assert_eq!(json(&vanilla, &with_dictionaries(&["A", "b"])), json!({}));
}

#[test]
fn physics_dictionaries_are_rebuilt_exactly() {
  let vanilla = with_dictionaries(&["a", "b", "c"]);
  let modded = with_dictionaries(&["d", "a", "e", "c"]);
  let mut applied = vanilla.clone();
  applied.apply(&diff(&vanilla, &modded)).unwrap();
  assert_eq!(applied.physics_dictionaries, modded.physics_dictionaries);
}
