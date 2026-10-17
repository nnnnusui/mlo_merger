use serde_json::json;

use super::*;
use crate::core::format::ymap::compare::*;
use crate::core::format::ymap::model::YmapCarGenerator;

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

fn with_generators(car_generators: Vec<YmapCarGenerator>) -> Ymap {
  Ymap {
    car_generators,
    ..base()
  }
}

#[test]
fn car_generators_pair_by_rounded_position() {
  let vanilla = with_generators(vec![generator(10.0), generator(20.0)]);
  let mut modded = vanilla.clone();
  modded.car_generators[0].orient_y = 0.5;
  modded.car_generators[0].car_model = "taxi".into();
  modded.car_generators.remove(1);
  assert_eq!(
    json(&vanilla, &modded),
    json!({
      "car_generators": [
        {
          "Modified": {
            "vanilla_index": 0,
            "modded_index": 0,
            "changes": { "orient_y": 0.5, "car_model": "taxi" },
          },
        },
        { "Removed": { "index": 1 } },
      ],
    })
  );
}

#[test]
fn car_generator_pairing_follows_the_configured_decimals() {
  let vanilla = with_generators(vec![generator(10.0)]);
  let modded = with_generators(vec![generator(10.25)]);
  // Three decimals tell 10.25 from 10.0; none round both to 10.
  assert_eq!(diff(&vanilla, &modded).car_generators.unwrap_or_default().len(), 2);
  let coarse = MatchTolerances {
    ymap_car_generator_position_decimals: 0,
    ..Default::default()
  };
  assert_eq!(
    serde_json::to_value(diff_with(&vanilla, &modded, &coarse)).unwrap(),
    json!({
      "car_generators": [{
        "Modified": { "vanilla_index": 0, "modded_index": 0, "changes": { "position": { "x": 10.25 } } },
      }],
    })
  );
}

// Moved from `ymap_car_generator_diff.rs`: the diff function on its own.

#[test]
fn pairs_by_rounded_position_and_thresholds_floats() {
  let vanilla = Ymap {
    car_generators: vec![generator(10.0), generator(20.0)],
    ..base()
  };
  let mut modded = vanilla.clone();
  modded.car_generators[0].orient_x += 0.0005;
  modded.car_generators[1].position.x = 20.01;
  modded.car_generators[1].car_model = "taxi".into();

  // Three decimals keep 20.01 apart from 20.0.
  let diffs = compare::car_generators::diff(
    &vanilla.car_generators,
    &modded.car_generators,
    &MatchTolerances::default(),
  )
  .unwrap();
  assert_eq!(
    diffs,
    vec![
      YmapCarGeneratorDiff::Removed {
        index: 1
      },
      YmapCarGeneratorDiff::Added {
        index: 1,
        item: modded.car_generators[1].clone(),
      },
    ]
  );
  // One decimal pairs them, reporting the moved position and the model.
  let coarse = MatchTolerances {
    ymap_car_generator_position_decimals: 1,
    ..Default::default()
  };
  assert_eq!(
    compare::car_generators::diff(&vanilla.car_generators, &modded.car_generators, &coarse)
      .unwrap(),
    vec![YmapCarGeneratorDiff::Modified {
      vanilla_index: 1,
      modded_index: 1,
      changes: YmapCarGeneratorFieldDiff {
        position: Some(PositionDiff {
          x: Some(20.01),
          ..Default::default()
        }),
        car_model: Some("taxi".into()),
        ..Default::default()
      },
    }]
  );
}
