use serde::Deserialize;

use crate::core::format::{
  xml::{XmlValueAttr, position::XmlPositionAttr},
  ymap::model::YmapCarGenerator,
};

#[derive(Debug, Deserialize)]
#[serde(rename = "Item")]
pub struct XmlYmapCarGenerator {
  pub position: XmlPositionAttr,
  #[serde(rename = "orientX")]
  pub orient_x: XmlValueAttr<f32>,
  #[serde(rename = "orientY")]
  pub orient_y: XmlValueAttr<f32>,
  #[serde(rename = "perpendicularLength")]
  pub perpendicular_length: XmlValueAttr<f32>,
  #[serde(rename = "carModel", default)]
  pub car_model: String,
  pub flags: XmlValueAttr<u32>,
  #[serde(rename = "bodyColorRemap1")]
  pub body_color_remap_1: XmlValueAttr<i32>,
  #[serde(rename = "bodyColorRemap2")]
  pub body_color_remap_2: XmlValueAttr<i32>,
  #[serde(rename = "bodyColorRemap3")]
  pub body_color_remap_3: XmlValueAttr<i32>,
  #[serde(rename = "bodyColorRemap4")]
  pub body_color_remap_4: XmlValueAttr<i32>,
  #[serde(rename = "popGroup", default)]
  pub pop_group: String,
  pub livery: XmlValueAttr<i32>,
}

impl From<XmlYmapCarGenerator> for YmapCarGenerator {
  fn from(v: XmlYmapCarGenerator) -> Self {
    Self {
      position: v.position.into(),
      orient_x: v.orient_x.value,
      orient_y: v.orient_y.value,
      perpendicular_length: v.perpendicular_length.value,
      car_model: v.car_model,
      flags: v.flags.value,
      body_color_remap_1: v.body_color_remap_1.value,
      body_color_remap_2: v.body_color_remap_2.value,
      body_color_remap_3: v.body_color_remap_3.value,
      body_color_remap_4: v.body_color_remap_4.value,
      pop_group: v.pop_group,
      livery: v.livery.value,
    }
  }
}
