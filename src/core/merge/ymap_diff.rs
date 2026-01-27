use crate::core::{
  format::ymap::model::{Ymap, YmapStructDiffEnum},
  merge::{
    ymap_box_occluder_diff::YmapBoxOccluderDiff, ymap_entitiy_diff::YmapEntityDiff,
    ymap_occlude_model_diff::YmapOccludeModelDiff,
  },
};
use structdiff::StructDiff;

pub struct YmapDiff {
  pub entity_diffs: Vec<YmapEntityDiff>,
  pub box_occluder_diffs: Vec<YmapBoxOccluderDiff>,
  pub occlude_model_diffs: Vec<YmapOccludeModelDiff>,
}

impl YmapDiff {
  pub fn extract_from(
    vanilla: &Ymap,
    modded: &Ymap,
  ) -> Self {
    let diffs = vanilla.diff(modded);
    for diff in &diffs {
      match diff {
        YmapStructDiffEnum::entity_map(_) => {}
        YmapStructDiffEnum::box_occluders(_) => {}
        YmapStructDiffEnum::occlude_models(_) => {}
        it => log::warn!("    [warning] skip unsupported changes: {:?}", it),
      }
    }
    let entity_diffs = YmapEntityDiff::extract_from(vanilla, modded);
    YmapEntityDiff::print_diffs(&entity_diffs);
    let box_occluder_diffs = YmapBoxOccluderDiff::extract_from(vanilla, modded);
    YmapBoxOccluderDiff::print_diffs(&box_occluder_diffs);
    let occlude_model_diffs = YmapOccludeModelDiff::extract_from(vanilla, modded);
    YmapOccludeModelDiff::print_diffs(&occlude_model_diffs);

    Self {
      entity_diffs,
      box_occluder_diffs,
      occlude_model_diffs,
    }
  }
}
