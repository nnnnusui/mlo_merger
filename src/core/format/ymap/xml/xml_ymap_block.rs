use crate::core::format::{xml::XmlValueAttr, ymap::model::YmapBlock};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XmlYmapBlock {
  pub version: XmlValueAttr<u32>,
  pub flags: XmlValueAttr<u32>,
  pub name: String,
  pub exported_by: String,
  pub owner: String,
  pub time: String,
}

impl From<XmlYmapBlock> for YmapBlock {
  fn from(v: XmlYmapBlock) -> Self {
    Self {
      version: v.version.value,
      flags: v.flags.value,
      name: v.name,
      exported_by: v.exported_by,
      owner: v.owner,
      time: v.time,
    }
  }
}
