use serde::Deserialize;

use crate::core::format::{
  xml::{XmlValueAttr, position::XmlPositionAttr},
  ymap::model::YmapTimeCycleModifier,
};

#[derive(Debug, Deserialize)]
#[serde(rename = "Item")]
pub struct XmlYmapTimeCycleModifier {
  pub name: String,
  #[serde(rename = "minExtents")]
  pub min_extents: XmlPositionAttr,
  #[serde(rename = "maxExtents")]
  pub max_extents: XmlPositionAttr,
  pub percentage: XmlValueAttr<u32>,
  pub range: XmlValueAttr<f32>,
  #[serde(rename = "startHour")]
  pub start_hour: XmlValueAttr<u32>,
  #[serde(rename = "endHour")]
  pub end_hour: XmlValueAttr<u32>,
}

impl From<XmlYmapTimeCycleModifier> for YmapTimeCycleModifier {
  fn from(v: XmlYmapTimeCycleModifier) -> Self {
    Self {
      name: v.name,
      min_extents: v.min_extents.into(),
      max_extents: v.max_extents.into(),
      percentage: v.percentage.value,
      range: v.range.value,
      start_hour: v.start_hour.value,
      end_hour: v.end_hour.value,
    }
  }
}
