use serde::Deserialize;

use crate::common::Position;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XmlPositionAttr {
  #[serde(rename = "@x")]
  x: f32,
  #[serde(rename = "@y")]
  y: f32,
  #[serde(rename = "@z")]
  z: f32,
}

impl From<XmlPositionAttr> for Position {
  fn from(v: XmlPositionAttr) -> Self {
    Self { x: v.x, y: v.y, z: v.z }
  }
}
