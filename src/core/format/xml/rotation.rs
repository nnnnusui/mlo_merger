use serde::Deserialize;

use crate::core::common::Rotation;

#[derive(Debug, Deserialize)]
pub struct XmlRotation {
  #[serde(rename = "@x")]
  pub x: f32,
  #[serde(rename = "@y")]
  pub y: f32,
  #[serde(rename = "@z")]
  pub z: f32,
  #[serde(rename = "@w")]
  pub w: f32,
}

impl From<XmlRotation> for Rotation {
  fn from(v: XmlRotation) -> Self {
    Self {
      x: v.x,
      y: v.y,
      z: v.z,
      w: v.w,
    }
  }
}
