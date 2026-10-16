use serde_json::json;

use super::*;
use crate::core::format::ymap::compare::*;

fn with_dictionaries(names: &[&str]) -> Ymap {
  Ymap {
    physics_dictionaries: names.iter().map(|name| name.to_string()).collect(),
    ..base()
  }
}

#[test]
fn only_added_physics_dictionaries_are_recorded() {
  let vanilla = with_dictionaries(&["a", "b"]);
  assert_eq!(
    json(&vanilla, &with_dictionaries(&["c", "b"])),
    json!({ "physics_dictionaries": ["c"] })
  );
  // Removals and reordering are not differences.
  assert_eq!(json(&vanilla, &with_dictionaries(&["b"])), json!({}));
  assert_eq!(json(&vanilla, &with_dictionaries(&["b", "a"])), json!({}));
}

// Moved from `diff.rs`: the function on its own.

#[test]
fn records_only_added_physics_dictionaries() {
  let vanilla = Ymap {
    physics_dictionaries: vec!["a".into(), "b".into()],
    ..base()
  };
  let modded = Ymap {
    physics_dictionaries: vec!["c".into(), "b".into()],
    ..base()
  };
  assert_eq!(added_physics_dictionaries(&vanilla, &modded), vec!["c".to_string()]);
  assert!(
    added_physics_dictionaries(
      &modded,
      &Ymap {
        ..base()
      }
    )
    .is_empty()
  );
}
