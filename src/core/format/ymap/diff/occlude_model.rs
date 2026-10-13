use crate::{
  continue_early,
  core::{
    common::triangle::Triangle,
    format::ymap::model::{
      Ymap, YmapOccludeModel, ymap_occlude_model::YmapOccludeModelStructDiffEnum,
    },
  },
};

#[derive(serde::Serialize, serde::Deserialize)]
pub enum YmapOccludeModelDiff {
  Added(YmapOccludeModel),
  Removed(YmapOccludeModel),
  Modified {
    vanilla: YmapOccludeModel,
    diffs: Vec<YmapOccludeModelStructDiffEnum>,
    triangle_diffs: Vec<YmapOccludeModelTriangleDiff>,
  },
}

impl YmapOccludeModelDiff {
  pub fn extract_from(
    vanilla: &Ymap,
    modded: &Ymap,
  ) -> Vec<YmapOccludeModelDiff> {
    check_occlude_models_diff(vanilla, modded)
  }

  /// Extracts merge changes without copying unchanged triangles into Removed/Modified baselines.
  pub(crate) fn extract_for_merge(
    vanilla: &Ymap,
    modded: &Ymap,
  ) -> Vec<YmapOccludeModelDiff> {
    check_occlude_models_diff_with_options(vanilla, modded, true)
  }

  pub fn print_diffs(diffs: &Vec<YmapOccludeModelDiff>) {
    let _span = tracing::info_span!("occlude_models").entered();
    tracing::info!("Found {} differences", diffs.len());
    for diff in diffs {
      match diff {
        Self::Added(m) => {
          tracing::info!("[Added] Occlude Model: bmin={:?}, bmax={:?}", m.bmin, m.bmax);
        }
        Self::Removed(m) => {
          tracing::info!("[Removed] Occlude Model: bmin={:?}, bmax={:?}", m.bmin, m.bmax);
        }
        Self::Modified {
          vanilla: _,
          diffs,
          triangle_diffs,
        } => {
          tracing::info!("[Modified] Diffs: {:?}", diffs);
          continue_early!(if (triangle_diffs.is_empty()) continue);

          let added_count = triangle_diffs
            .iter()
            .filter(|d| matches!(d, YmapOccludeModelTriangleDiff::Added(_)))
            .count();
          let removed_count = triangle_diffs
            .iter()
            .filter(|d| matches!(d, YmapOccludeModelTriangleDiff::Removed(_)))
            .count();
          let _triangle_span = tracing::info_span!("triangle_diffs").entered();
          tracing::info!("{} Added, {} Removed", added_count, removed_count);
        }
      }
    }
  }
}

pub fn check_occlude_models_diff(
  vanilla_ymap: &Ymap,
  mod_ymap: &Ymap,
) -> Vec<YmapOccludeModelDiff> {
  check_occlude_models_diff_with_options(vanilla_ymap, mod_ymap, false)
}

fn check_occlude_models_diff_with_options(
  vanilla_ymap: &Ymap,
  mod_ymap: &Ymap,
  omit_baseline_triangles: bool,
) -> Vec<YmapOccludeModelDiff> {
  let vanilla = &vanilla_ymap.occlude_models;
  let modified = &mod_ymap.occlude_models;

  let mut diffs = Vec::new();

  // Check for removed and modified items
  for vanilla_item in vanilla {
    if let Some(mod_item) = modified.iter().find(|m| vanilla_item.is_same(m)) {
      // Compare scalar metadata separately to avoid cloning the large triangles field.
      let item_diffs = check_occlude_model_fields_diff(vanilla_item, mod_item);
      let triangle_diffs = check_triangles_diff(vanilla_item, mod_item);
      continue_early!(if (item_diffs.is_empty() && triangle_diffs.is_empty()) continue);

      diffs.push(YmapOccludeModelDiff::Modified {
        vanilla: baseline_snapshot(vanilla_item, omit_baseline_triangles),
        diffs: item_diffs,
        triangle_diffs,
      });
    } else {
      // Item only in vanilla
      diffs.push(YmapOccludeModelDiff::Removed(baseline_snapshot(
        vanilla_item,
        omit_baseline_triangles,
      )));
    }
  }

  // Check for added items
  for mod_item in modified {
    if !vanilla.iter().any(|v| v.is_same(mod_item)) {
      diffs.push(YmapOccludeModelDiff::Added(mod_item.clone()));
    }
  }

  diffs
}

fn check_occlude_model_fields_diff(
  vanilla: &YmapOccludeModel,
  modded: &YmapOccludeModel,
) -> Vec<YmapOccludeModelStructDiffEnum> {
  let mut diffs = Vec::new();
  if vanilla.bmin != modded.bmin {
    diffs.push(YmapOccludeModelStructDiffEnum::bmin(modded.bmin.clone()));
  }
  if vanilla.bmax != modded.bmax {
    diffs.push(YmapOccludeModelStructDiffEnum::bmax(modded.bmax.clone()));
  }
  if vanilla.flags != modded.flags {
    diffs.push(YmapOccludeModelStructDiffEnum::flags(modded.flags));
  }
  diffs
}

fn baseline_snapshot(
  model: &YmapOccludeModel,
  omit_triangles: bool,
) -> YmapOccludeModel {
  if omit_triangles {
    YmapOccludeModel {
      bmin: model.bmin.clone(),
      bmax: model.bmax.clone(),
      triangles: Vec::new(),
      flags: model.flags,
    }
  } else {
    model.clone()
  }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum YmapOccludeModelTriangleDiff {
  Added(Triangle),
  Removed(Triangle),
}

fn check_triangles_diff(
  vanilla: &YmapOccludeModel,
  modded: &YmapOccludeModel,
) -> Vec<YmapOccludeModelTriangleDiff> {
  let mut diffs = Vec::new();

  // Check for removed triangles
  for vanilla_triangle in &vanilla.triangles {
    if !modded.triangles.contains(vanilla_triangle) {
      diffs.push(YmapOccludeModelTriangleDiff::Removed(vanilla_triangle.clone()));
    }
  }

  // Check for added triangles
  for modded_triangle in &modded.triangles {
    if !vanilla.triangles.contains(modded_triangle) {
      diffs.push(YmapOccludeModelTriangleDiff::Added(modded_triangle.clone()));
    }
  }

  diffs
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::common::{position::Position, triangle::Triangle};

  fn model(triangles: Vec<Triangle>) -> YmapOccludeModel {
    YmapOccludeModel {
      bmin: Position::default(),
      bmax: Position {
        x: 10.0,
        y: 10.0,
        z: 10.0,
      },
      triangles,
      flags: 0,
    }
  }

  fn triangle(offset: f32) -> Triangle {
    Triangle {
      corner_1: Position {
        x: offset,
        y: 0.0,
        z: 0.0,
      },
      corner_2: Position {
        x: offset + 1.0,
        y: 0.0,
        z: 0.0,
      },
      corner_3: Position {
        x: offset,
        y: 1.0,
        z: 0.0,
      },
    }
  }

  #[test]
  fn merge_diff_keeps_triangle_deltas_but_omits_baseline_triangle_copies() {
    let unchanged = triangle(0.0);
    let removed = triangle(2.0);
    let added = triangle(4.0);
    let vanilla = Ymap {
      occlude_models: vec![model(vec![unchanged.clone(), removed.clone()])],
      ..empty_ymap()
    };
    let source = Ymap {
      occlude_models: vec![YmapOccludeModel {
        bmin: Position {
          x: 0.0,
          y: 0.0,
          z: 0.5,
        },
        triangles: vec![unchanged.clone(), added.clone()],
        flags: 1,
        ..model(Vec::new())
      }],
      ..empty_ymap()
    };
    let full = check_occlude_models_diff(&vanilla, &source);
    let merge = YmapOccludeModelDiff::extract_for_merge(&vanilla, &source);
    let YmapOccludeModelDiff::Modified {
      vanilla: full_baseline,
      ..
    } = &full[0]
    else {
      panic!("Expected full diff to modify the model")
    };
    let YmapOccludeModelDiff::Modified {
      vanilla: merge_baseline,
      diffs,
      triangle_diffs,
      ..
    } = &merge[0]
    else {
      panic!("Expected merge diff to modify the model")
    };
    assert_eq!(full_baseline.triangles.len(), 2);
    assert!(merge_baseline.triangles.is_empty());
    assert_eq!(triangle_diffs.len(), 2);
    assert!(triangle_diffs.iter().any(|diff| matches!(
      diff,
      YmapOccludeModelTriangleDiff::Removed(triangle) if triangle == &removed
    )));
    assert!(triangle_diffs.iter().any(|diff| matches!(
      diff,
      YmapOccludeModelTriangleDiff::Added(triangle) if triangle == &added
    )));
    assert!(diffs.iter().any(|diff| matches!(
      diff,
      YmapOccludeModelStructDiffEnum::bmin(position) if position == &source.occlude_models[0].bmin
    )));
    assert!(diffs.iter().any(|diff| matches!(
      diff,
      YmapOccludeModelStructDiffEnum::flags(flags) if *flags == source.occlude_models[0].flags
    )));

    let removed_diff = YmapOccludeModelDiff::extract_for_merge(
      &vanilla,
      &Ymap {
        occlude_models: Vec::new(),
        ..empty_ymap()
      },
    );
    let YmapOccludeModelDiff::Removed(removed_baseline) = &removed_diff[0] else {
      panic!("Expected removed model")
    };
    assert!(removed_baseline.triangles.is_empty());

    let added_diff = YmapOccludeModelDiff::extract_for_merge(
      &Ymap {
        occlude_models: Vec::new(),
        ..empty_ymap()
      },
      &source,
    );
    let YmapOccludeModelDiff::Added(added_model) = &added_diff[0] else {
      panic!("Expected added model")
    };
    assert_eq!(added_model.triangles.len(), 2);
  }

  #[test]
  fn merge_diff_without_baseline_triangles_applies_to_the_exact_model() {
    let vanilla = Ymap {
      occlude_models: vec![model(vec![triangle(0.0), triangle(2.0)])],
      ..empty_ymap()
    };
    let source = Ymap {
      occlude_models: vec![model(vec![triangle(0.0), triangle(4.0)])],
      ..empty_ymap()
    };
    let diff = crate::core::format::ymap::diff::YmapDiff::extract_for_merge(&vanilla, &source);

    let merged = diff.apply_to(&vanilla, None);

    assert_eq!(merged.occlude_models, source.occlude_models);
  }

  fn empty_ymap() -> Ymap {
    use crate::core::format::ymap::model::{YmapBlock, YmapDistantLodLights, YmapInstancedData};
    Ymap {
      name: String::new(),
      parent: String::new(),
      flags: 0,
      content_flags: 0,
      streaming_extents_min: Position::default(),
      streaming_extents_max: Position::default(),
      entities_extents_min: Position::default(),
      entities_extents_max: Position::default(),
      entities: Vec::new(),
      box_occluders: Vec::new(),
      occlude_models: Vec::new(),
      lod_lights: Vec::new(),
      distant_lod_lights: YmapDistantLodLights::default(),
      block: YmapBlock {
        version: 0,
        flags: 0,
        name: String::new(),
        exported_by: String::new(),
        owner: String::new(),
        time: String::new(),
      },
      container_lods: Vec::new(),
      physics_dictionaries: Vec::new(),
      instanced_data: YmapInstancedData::default(),
      time_cycle_modifiers: Vec::new(),
      car_generators: Vec::new(),
    }
  }
}
