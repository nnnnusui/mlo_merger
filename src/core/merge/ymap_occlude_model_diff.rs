use crate::{
  continue_early,
  core::{
    common::triangle::Triangle,
    format::ymap::model::{
      Ymap, YmapOccludeModel, ymap_occlude_model::YmapOccludeModelStructDiffEnum,
    },
  },
};
use structdiff::StructDiff;

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

  pub fn print_diffs(diffs: &Vec<YmapOccludeModelDiff>) {
    log::info!("    Found {} occlude model  differences", diffs.len());
    for diff in diffs {
      match diff {
        Self::Added(m) => {
          log::info!("      [Added] Occlude Model: bmin={:?}, bmax={:?}", m.bmin, m.bmax);
        }
        Self::Removed(m) => {
          log::info!("      [Removed] Occlude Model: bmin={:?}, bmax={:?}", m.bmin, m.bmax);
        }
        Self::Modified {
          vanilla: _,
          diffs,
          triangle_diffs,
        } => {
          log::info!("      [Modified] Diffs: {:?}", diffs);
          continue_early!(if (triangle_diffs.is_empty()) continue);

          let added_count = triangle_diffs
            .iter()
            .filter(|d| matches!(d, YmapOccludeModelTriangleDiff::Added(_)))
            .count();
          let removed_count = triangle_diffs
            .iter()
            .filter(|d| matches!(d, YmapOccludeModelTriangleDiff::Removed(_)))
            .count();
          log::info!("        Triangle Diffs: {} Added, {} Removed", added_count, removed_count);
        }
      }
    }
  }
}

pub fn check_occlude_models_diff(
  vanilla_ymap: &Ymap,
  mod_ymap: &Ymap,
) -> Vec<YmapOccludeModelDiff> {
  let vanilla = &vanilla_ymap.occlude_models;
  let modified = &mod_ymap.occlude_models;

  let mut diffs = Vec::new();

  // Check for removed and modified items
  for vanilla_item in vanilla {
    if let Some(mod_item) = modified.iter().find(|m| vanilla_item.is_same(m)) {
      // Item exists in both, check if modified
      let item_diffs = vanilla_item.diff(mod_item);
      continue_early!(if (item_diffs.is_empty()) continue);

      let triangle_diffs = check_triangles_diff(vanilla_item, mod_item);
      // triangles の diff は triangle_diffs に分離するため、item_diffs から除外
      let item_diffs: Vec<_> = item_diffs
        .into_iter()
        .filter(|diff| !matches!(diff, YmapOccludeModelStructDiffEnum::triangles(_)))
        .collect();

      diffs.push(YmapOccludeModelDiff::Modified {
        vanilla: vanilla_item.clone(),
        diffs: item_diffs,
        triangle_diffs,
      });
    } else {
      // Item only in vanilla
      diffs.push(YmapOccludeModelDiff::Removed(vanilla_item.clone()));
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
