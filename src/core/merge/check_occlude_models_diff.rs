use crate::core::format::ymap::model::{
  Ymap, YmapOccludeModel, ymap_occlude_model::YmapOccludeModelStructDiffEnum,
};

pub enum YmapOccludeModelsDiff {
  Added(YmapOccludeModel),
  Removed(YmapOccludeModel),
  Modified {
    vanilla: YmapOccludeModel,
    diffs: Vec<YmapOccludeModelStructDiffEnum>,
  },
}

pub fn check_occlude_models_diff(
  vanilla_ymap: &Ymap,
  mod_ymap: &Ymap,
) -> Vec<YmapOccludeModelsDiff> {
  // Currently unimplemented
  vec![]
}
