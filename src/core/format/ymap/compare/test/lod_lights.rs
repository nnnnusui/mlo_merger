use serde_json::json;

use super::*;
use crate::core::format::ymap::compare::*;
use crate::core::format::ymap::model::YmapLodLight;

fn light(falloff: f32) -> YmapLodLight {
  YmapLodLight {
    falloff,
    hash: "0".into(),
    ..YmapLodLight::default()
  }
}

#[test]
fn lod_lights_compare_as_a_multiset_without_parent_positions() {
  let vanilla = Ymap {
    lod_lights: vec![light(1.0), light(2.0), light(3.0)],
    ..base()
  };
  // Dropping the first light shifts the rest, which must not read as modifications.
  let modded = Ymap {
    lod_lights: vec![light(2.0), light(3.0), light(9.0)],
    ..base()
  };
  assert_eq!(
    json(&vanilla, &modded),
    json!({
      "lod_lights": [
        { "Removed": { "base_index": 0 } },
        { "Added": { "anchor_base_index": 2, "target_index": 2, "item": serde_json::to_value(light(9.0)).unwrap() } },
      ],
    })
  );
}

// Moved from `ymap_lod_light_diff.rs`: the diff function on its own.

#[test]
fn compares_lights_as_a_multiset() {
  let vanilla = Ymap {
    lod_lights: vec![light(1.0), light(2.0), light(3.0), light(2.0)],
    ..base()
  };
  // The source drops the first light and one duplicate, and edits the third.
  let modded = Ymap {
    lod_lights: vec![light(2.0), light(9.0)],
    ..base()
  };
  assert_eq!(
    vanilla.lod_lights.diff(&modded.lod_lights, &()).unwrap(),
    vec![
      YmapLodLightDiff::Removed {
        base_index: 0,
        implied: false,
        reason: None,
      },
      YmapLodLightDiff::Removed {
        base_index: 3,
        implied: false,
        reason: None,
      },
      // The edited light takes the place of the first light the source lacks after it.
      YmapLodLightDiff::Replaced {
        base_index: 2,
        target_index: Some(1),
        item: light(9.0),
      },
    ]
  );
  assert_eq!(vanilla.lod_lights.diff(&vanilla.lod_lights, &()), None);
}
