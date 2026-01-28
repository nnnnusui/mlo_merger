use serde::{Deserialize, Serialize};

use crate::core::common::rotation::Rotation;

#[derive(Debug, Deserialize, Serialize)]
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

impl From<Rotation> for XmlRotation {
  fn from(v: Rotation) -> Self {
    Self {
      x: v.x,
      y: v.y,
      z: v.z,
      w: v.w,
    }
  }
}
