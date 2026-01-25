use serde::Deserialize;

use crate::core::format::{xml::XmlValueAttr, ymap::model::YmapBoxOccluder};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XmlYmapBoxOccluder {
  pub i_center_x: XmlValueAttr<i32>,
  pub i_center_y: XmlValueAttr<i32>,
  pub i_center_z: XmlValueAttr<i32>,
  pub i_cos_z: XmlValueAttr<i32>,
  pub i_length: XmlValueAttr<u32>,
  pub i_width: XmlValueAttr<u32>,
  pub i_height: XmlValueAttr<u32>,
  pub i_sin_z: XmlValueAttr<i32>,
}

impl From<XmlYmapBoxOccluder> for YmapBoxOccluder {
  fn from(v: XmlYmapBoxOccluder) -> Self {
    Self {
      i_center_x: v.i_center_x.value,
      i_center_y: v.i_center_y.value,
      i_center_z: v.i_center_z.value,
      i_cos_z: v.i_cos_z.value,
      i_length: v.i_length.value,
      i_width: v.i_width.value,
      i_height: v.i_height.value,
      i_sin_z: v.i_sin_z.value,
    }
  }
}
