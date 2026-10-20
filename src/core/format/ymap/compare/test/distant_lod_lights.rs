use serde_json::json;

use super::*;
use crate::core::format::ymap::compare::*;
use crate::core::format::ymap::model::YmapDistantLodLight;

fn light(
  position: &str,
  rgbi: &str,
) -> YmapDistantLodLight {
  YmapDistantLodLight {
    position: position.into(),
    rgbi: rgbi.into(),
  }
}

fn with_lights(
  items: Vec<YmapDistantLodLight>,
  num_street_lights: u32,
) -> Ymap {
  Ymap {
    distant_lod_lights: YmapDistantLodLights {
      items,
      num_street_lights,
      category: 0,
    },
    ..base()
  }
}

#[test]
fn distant_lights_pair_by_position_and_report_block_fields() {
  let vanilla =
    with_lights(vec![light("1 2 3", "10"), light("4 5 6", "20"), light("7 8 9", "30")], 2);
  let modded =
    with_lights(vec![light("4.0004 5 6", "20"), light("1 2 3", "11"), light("10 11 12", "40")], 3);
  assert_eq!(
    json(&vanilla, &modded),
    json!({
      "distant_lod_lights": {
        "num_street_lights": 3,
        "items": [
          { "Modified": { "before_index": 0, "after_index": 1, "changes": { "rgbi": "11" } } },
          { "Removed": { "index": 2 } },
          { "Added": { "after": 0, "item": { "position": "10 11 12", "rgbi": "40" } } },
        ],
      },
    })
  );
}

#[test]
fn distant_lights_moved_within_the_rounding_report_the_position() {
  let vanilla = with_lights(vec![light("1 2 3", "10")], 0);
  let modded = with_lights(vec![light("1.0002 2 3.25", "10")], 0);
  // 3.25 rounds apart from 3 at three decimals, so the light no longer pairs and is replaced.
  assert!(matches!(
    diff(&vanilla, &modded).distant_lod_lights.unwrap().items.unwrap().as_slice(),
    [YmapDistantLodLightDiff::Replaced {
      index: 0,
      ..
    }]
  ));
  let coarse = MatchTolerances {
    ymap_lod_light_position_decimals: 0,
    ..Default::default()
  };
  assert_eq!(
    serde_json::to_value(diff_with(&vanilla, &modded, &coarse)).unwrap(),
    json!({
      "distant_lod_lights": {
        "items": [{
          "Modified": { "before_index": 0, "after_index": 0, "changes": { "position": { "z": 3.25 } } },
        }],
      },
    })
  );
}

// Moved from `ymap_distant_lod_light_diff.rs`: the diff function on its own.

fn with_category(
  items: Vec<YmapDistantLodLight>,
  category: u32,
) -> Ymap {
  Ymap {
    distant_lod_lights: YmapDistantLodLights {
      items,
      num_street_lights: 0,
      category,
    },
    ..base()
  }
}

#[test]
fn pairs_by_rounded_position_and_reports_block_fields() {
  let vanilla =
    with_category(vec![light("1 2 3", "a"), light("4 5 6", "b"), light("7 8 9", "c")], 0);
  let modded =
    with_category(vec![light("4.0002 5 6", "b"), light("1 2 3", "z"), light("10 11 12", "d")], 2);
  let diff = vanilla
    .distant_lod_lights
    .diff(&modded.distant_lod_lights, &MatchTolerances::default())
    .unwrap();
  assert_eq!(diff.category, Some(2));
  assert_eq!(diff.num_street_lights, None);
  assert_eq!(
    diff.items.unwrap(),
    vec![
      YmapDistantLodLightDiff::Modified {
        before_index: 0,
        after_index: 1,
        changes: YmapDistantLodLightFieldDiff {
          rgbi: Some("z".into()),
          ..Default::default()
        },
      },
      YmapDistantLodLightDiff::Removed {
        index: 2,
        implied: false,
      },
      // After the light the source puts before it, vanilla's first.
      YmapDistantLodLightDiff::Added {
        after: Some(0),
        item: light("10 11 12", "d"),
      },
    ]
  );
  assert_eq!(
    vanilla.distant_lod_lights.diff(&vanilla.distant_lod_lights, &MatchTolerances::default()),
    None
  );
}
