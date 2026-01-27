use crate::core::format::ymap::model::YmapOccludeModel;

pub enum YmapOccludeModelsDiff {
  Added(YmapOccludeModel),
  Removed(YmapOccludeModel),
  Modified {
    vanilla: YmapOccludeModel,
    modded: YmapOccludeModel,
  },
}
