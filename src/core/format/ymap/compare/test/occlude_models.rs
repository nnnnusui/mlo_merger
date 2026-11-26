//! Occlude models diffed and applied as the triangles of the whole YMAP.

use serde_json::json;

use super::*;
use crate::{
  common::triangle::Triangle,
  core::format::ymap::model::{YmapOccludeModel, ymap_occlude_model::MAX_OCCLUDE_MODEL_VERTICES},
};

/// A triangle at `x` facing +Z.
fn triangle(x: f32) -> Triangle {
  Triangle {
    corner_1: at(x, 0.0, 0.0),
    corner_2: at(x + 1.0, 0.0, 0.0),
    corner_3: at(x, 1.0, 0.0),
  }
}

/// `triangle` with its corners rotated, the same triangle.
fn rotated(x: f32) -> Triangle {
  let Triangle {
    corner_1,
    corner_2,
    corner_3,
  } = triangle(x);
  Triangle {
    corner_1: corner_2,
    corner_2: corner_3,
    corner_3: corner_1,
  }
}

/// `triangle` facing -Z.
fn reversed(x: f32) -> Triangle {
  let Triangle {
    corner_1,
    corner_2,
    corner_3,
  } = triangle(x);
  Triangle {
    corner_1,
    corner_2: corner_3,
    corner_3: corner_2,
  }
}

/// A model of `triangles` with their exact bounds.
fn model(
  flags: u32,
  triangles: Vec<Triangle>,
) -> YmapOccludeModel {
  let mut model = YmapOccludeModel {
    bmin: at(0.0, 0.0, 0.0),
    bmax: at(0.0, 0.0, 0.0),
    triangles,
    flags,
  };
  model.fit_bounds();
  model
}

fn ymap(models: Vec<YmapOccludeModel>) -> Ymap {
  Ymap {
    occlude_models: models,
    ..base()
  }
}

/// `vanilla` with the diff of `modded` applied.
fn applied(
  vanilla: &Ymap,
  modded: &Ymap,
) -> Ymap {
  let mut ymap = vanilla.clone();
  ymap.apply(&diff(vanilla, modded)).unwrap();
  ymap
}

#[test]
fn triangles_removed_and_added_are_listed_per_flags() {
  let vanilla =
    ymap(vec![model(0, vec![triangle(0.0), triangle(2.0)]), model(1, vec![triangle(9.0)])]);
  let modded =
    ymap(vec![model(0, vec![triangle(0.0), triangle(4.0)]), model(1, vec![triangle(9.0)])]);
  let value = |triangle: Triangle| serde_json::to_value(triangle).unwrap();
  assert_eq!(
    json(&vanilla, &modded),
    json!({
      "occlude_models": [
        { "flags": 0, "removed": [value(triangle(2.0))], "added": [value(triangle(4.0))] },
      ],
    })
  );
}

#[test]
fn regrouped_rotated_and_rebounded_triangles_are_unchanged() {
  let vanilla =
    ymap(vec![model(0, vec![triangle(0.0), triangle(2.0)]), model(0, vec![triangle(4.0)])]);
  let mut modded =
    ymap(vec![model(0, vec![triangle(4.0), rotated(0.0)]), model(0, vec![triangle(2.0)])]);
  modded.occlude_models[1].bmax.z = 30.0;
  assert_eq!(diff(&vanilla, &modded).occlude_models, None);
}

#[test]
fn reversed_winding_is_another_triangle() {
  let vanilla = ymap(vec![model(0, vec![triangle(0.0)])]);
  let modded = ymap(vec![model(0, vec![triangle(0.0), reversed(0.0)])]);
  let diffs = diff(&vanilla, &modded).occlude_models.unwrap();
  assert_eq!(diffs[0].added, vec![reversed(0.0)]);
  assert!(diffs[0].removed.is_empty());
}

#[test]
fn corners_within_the_tolerance_are_one_vertex_across_a_cell_boundary() {
  let near = |x: f32| Triangle {
    corner_1: at(x, 0.0, 0.0),
    corner_2: at(x + 1.0, 0.0, 0.0),
    corner_3: at(x, 1.0, 0.0),
  };
  // The default tolerance is 0.01: cells end at 0.01, between these two.
  let vanilla = ymap(vec![model(0, vec![near(0.0099)])]);
  let modded = ymap(vec![model(0, vec![near(0.0101)])]);
  assert_eq!(diff(&vanilla, &modded).occlude_models, None);
  let moved = ymap(vec![model(0, vec![near(0.03)])]);
  assert!(diff(&vanilla, &moved).occlude_models.is_some());
}

#[test]
fn exact_repeats_count_once() {
  let vanilla = ymap(vec![model(0, vec![triangle(0.0), triangle(0.0), triangle(2.0)])]);
  let modded = ymap(vec![model(0, vec![triangle(0.0), triangle(2.0)])]);
  assert_eq!(diff(&vanilla, &modded).occlude_models, None);
  // A model a diff changes keeps one of them: the added triangle joins it, sharing corners.
  let mut changed = modded.clone();
  changed.occlude_models[0].triangles.push(triangle(1.0));
  assert_eq!(
    applied(&vanilla, &changed).occlude_models[0].triangles,
    [triangle(0.0), triangle(2.0), triangle(1.0)]
  );
}

#[test]
fn removals_drop_emptied_models_and_refit_changed_ones() {
  let untouched = YmapOccludeModel {
    bmax: at(50.0, 50.0, 50.0),
    ..model(0, vec![triangle(20.0)])
  };
  let vanilla = ymap(vec![
    model(0, vec![triangle(0.0)]),
    untouched.clone(),
    model(0, vec![triangle(2.0), triangle(6.0)]),
  ]);
  let modded = ymap(vec![untouched.clone(), model(0, vec![triangle(2.0)])]);
  assert_eq!(applied(&vanilla, &modded).occlude_models, [untouched, model(0, vec![triangle(2.0)])]);
}

#[test]
fn connected_additions_join_the_model_sharing_most_vertices() {
  let vanilla = ymap(vec![model(0, vec![triangle(0.0)]), model(0, vec![triangle(10.0)])]);
  // Shares the corner at (11, 0, 0) with the second model.
  let joined = Triangle {
    corner_1: at(11.0, 0.0, 0.0),
    corner_2: at(12.0, 0.0, 0.0),
    corner_3: at(11.0, 1.0, 0.0),
  };
  let unlinked = triangle(40.0);
  let modded = ymap(vec![
    model(0, vec![triangle(0.0)]),
    model(0, vec![triangle(10.0), joined.clone(), unlinked.clone()]),
  ]);
  assert_eq!(
    applied(&vanilla, &modded).occlude_models,
    [
      model(0, vec![triangle(0.0)]),
      model(0, vec![triangle(10.0), joined]),
      model(0, vec![unlinked]),
    ]
  );
}

#[test]
fn a_piece_too_large_for_its_model_or_one_splits_into_connected_parts() {
  // A strip of quads: 2 vertices a quad, 2 triangles each.
  let strip = (0..200)
    .flat_map(|index| {
      let corner = |column: usize, z: f32| at(column as f32, 100.0, z);
      [
        Triangle {
          corner_1: corner(index, 0.0),
          corner_2: corner(index + 1, 0.0),
          corner_3: corner(index + 1, 1.0),
        },
        Triangle {
          corner_1: corner(index, 0.0),
          corner_2: corner(index + 1, 1.0),
          corner_3: corner(index, 1.0),
        },
      ]
    })
    .collect::<Vec<_>>();
  let vanilla = ymap(vec![model(0, vec![triangle(0.0)])]);
  let mut modded = vanilla.clone();
  modded.occlude_models.push(model(0, strip.clone()));
  let models = applied(&vanilla, &modded).occlude_models;
  assert_eq!(models[0], model(0, vec![triangle(0.0)]));
  let parts = &models[1..];
  assert_eq!(parts.len(), 2);
  assert_eq!(parts.iter().map(|part| part.triangles.len()).sum::<usize>(), strip.len());
  for part in parts {
    let vertices = part
      .triangles
      .iter()
      .flat_map(|t| [&t.corner_1, &t.corner_2, &t.corner_3])
      .fold(Vec::new(), |mut seen, corner| {
        if !seen.contains(&corner) {
          seen.push(corner);
        }
        seen
      });
    assert!(vertices.len() <= MAX_OCCLUDE_MODEL_VERTICES);
    assert_eq!(*part, model(0, part.triangles.clone()));
  }
  // The first part is the strip's start, taken breadth-first.
  assert_eq!(parts[0].triangles[..4], strip[..4]);
}

#[test]
fn an_addition_linked_to_a_full_model_makes_a_new_one() {
  let fan = |count: usize| {
    (0..count)
      .map(|index| Triangle {
        corner_1: at(0.0, 0.0, 0.0),
        corner_2: at(index as f32 + 1.0, 1.0, 0.0),
        corner_3: at(index as f32 + 2.0, 1.0, 0.0),
      })
      .collect::<Vec<_>>()
  };
  // 254 triangles use 1 + 255 vertices: one too many, so 253 fill the model.
  let full = fan(253);
  let vanilla = ymap(vec![model(0, full.clone())]);
  let modded = ymap(vec![model(0, fan(254))]);
  let models = applied(&vanilla, &modded).occlude_models;
  assert_eq!(models[0], model(0, full));
  assert_eq!(models[1], model(0, fan(254)[253..].to_vec()));
}

#[test]
fn a_flags_change_moves_the_triangles() {
  let vanilla = ymap(vec![model(0, vec![triangle(0.0)])]);
  let modded = ymap(vec![model(1, vec![triangle(0.0)])]);
  let diffs = diff(&vanilla, &modded).occlude_models.unwrap();
  assert_eq!(diffs.iter().map(|diff| diff.flags).collect::<Vec<_>>(), [0, 1]);
  assert_eq!(applied(&vanilla, &modded).occlude_models, modded.occlude_models);
}

#[test]
fn applying_a_sources_diff_holds_its_triangles() {
  let vanilla =
    ymap(vec![model(0, vec![triangle(0.0), triangle(2.0)]), model(1, vec![triangle(4.0)])]);
  let modded = ymap(vec![
    model(0, vec![triangle(2.0), triangle(6.0), reversed(6.0)]),
    model(1, vec![triangle(4.0), triangle(8.0)]),
    model(0, vec![triangle(30.0)]),
  ]);
  let applied = applied(&vanilla, &modded);
  assert_eq!(diff(&applied, &modded), YmapDiffFields::default());
}

#[test]
fn removing_a_missing_triangle_is_skipped() {
  let vanilla = ymap(vec![model(0, vec![triangle(0.0)])]);
  let diffs = diff(&vanilla, &ymap(vec![])).occlude_models.unwrap();
  let mut models = vec![model(0, vec![triangle(2.0)])];
  crate::core::format::ymap::compare::occlude_models::apply_with(&mut models, &diffs, 0.01)
    .unwrap();
  assert_eq!(models, [model(0, vec![triangle(2.0)])]);
}
