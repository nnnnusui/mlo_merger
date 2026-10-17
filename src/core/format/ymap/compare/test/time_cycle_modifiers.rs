use serde_json::json;

use super::*;
use crate::core::format::ymap::compare::*;
use crate::core::format::ymap::model::YmapTimeCycleModifier;

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
fn time_cycle_modifiers_pair_by_either_extent() {
  let vanilla = Ymap {
    time_cycle_modifiers: vec![modifier(0.0), modifier(100.0)],
    ..base()
  };
  let mut modded = Ymap {
    time_cycle_modifiers: vec![modifier(0.0), modifier(100.0), modifier(200.0)],
    ..base()
  };
  // The second keeps its minimum corner, so it pairs despite the moved maximum.
  modded.time_cycle_modifiers[1].max_extents.z = 30.5;
  modded.time_cycle_modifiers[1].name = "night".into();
  modded.time_cycle_modifiers[0].percentage += 0.0005;
  assert_eq!(
    json(&vanilla, &modded),
    json!({
      "time_cycle_modifiers": [
        {
          "Modified": {
            "vanilla_index": 1,
            "modded_index": 1,
            "changes": { "name": "night", "max_extents": { "z": 30.5 } },
          },
        },
        { "Added": { "after": 1, "item": serde_json::to_value(modifier(200.0)).unwrap() } },
      ],
    })
  );
}

// Moved from `ymap_time_cycle_modifier_diff.rs`: the diff function on its own.

#[test]
fn pairs_by_either_extent_and_thresholds_floats() {
  let vanilla = Ymap {
    time_cycle_modifiers: vec![modifier(0.0), modifier(100.0)],
    ..base()
  };
  let mut modded = vanilla.clone();
  modded.time_cycle_modifiers[0].percentage += 0.0005;
  modded.time_cycle_modifiers[1].max_extents.z = 30.0;
  modded.time_cycle_modifiers[1].end_hour = 6;
  modded.time_cycle_modifiers.push(modifier(200.0));

  let diffs = compare::time_cycle_modifiers::diff(
    &vanilla.time_cycle_modifiers,
    &modded.time_cycle_modifiers,
    &MatchTolerances::default(),
  )
  .unwrap();
  assert_eq!(
    diffs,
    vec![
      YmapTimeCycleModifierDiff::Modified {
        vanilla_index: 1,
        modded_index: 1,
        changes: YmapTimeCycleModifierFieldDiff {
          max_extents: Some(PositionDiff {
            z: Some(30.0),
            ..Default::default()
          }),
          end_hour: Some(6),
          ..Default::default()
        },
      },
      YmapTimeCycleModifierDiff::Added {
        after: Some(1),
        item: modifier(200.0),
      },
    ]
  );
}
