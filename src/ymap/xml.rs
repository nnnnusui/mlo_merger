use serde::Deserialize;

use crate::{xml::position::XmlPositionAttr, ymap::model::Ymap};

#[derive(Debug, Deserialize)]
#[serde(rename = "CMapData", rename_all = "camelCase", deny_unknown_fields)]
pub struct XmlYmap {
  pub name: String,
  pub parent: String,
  pub flags: YmapFlags,
  pub content_flags: YmapContentFlags,
  pub streaming_extents_min: XmlPositionAttr,
  pub streaming_extents_max: XmlPositionAttr,
  pub entities_extents_min: XmlPositionAttr,
  pub entities_extents_max: XmlPositionAttr,
}

impl From<XmlYmap> for Ymap {
  fn from(v: XmlYmap) -> Self {
    Self {
      name: v.name,
      parent: v.parent,
      flags: v.flags.value,
      content_flags: v.content_flags.value,
      streaming_extents_min: v.streaming_extents_min.into(),
      streaming_extents_max: v.streaming_extents_max.into(),
      entities_extents_min: v.entities_extents_min.into(),
      entities_extents_max: v.entities_extents_max.into(),
    }
  }
}

#[derive(Debug, Deserialize)]
pub struct YmapFlags {
  #[serde(rename = "@value")]
  pub value: u32,
}

#[derive(Debug, Deserialize)]
pub struct YmapContentFlags {
  #[serde(rename = "@value")]
  pub value: u32,
}
