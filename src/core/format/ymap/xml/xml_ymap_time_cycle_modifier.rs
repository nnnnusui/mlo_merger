use serde::{Deserialize, Serialize};

use crate::core::format::{
  xml::{XmlValueAttr, position::XmlPositionAttr},
  ymap::model::YmapTimeCycleModifier,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename = "Item")]
pub struct XmlYmapTimeCycleModifier {
  pub name: String,
  #[serde(rename = "minExtents")]
  pub min_extents: XmlPositionAttr,
  #[serde(rename = "maxExtents")]
  pub max_extents: XmlPositionAttr,
  pub percentage: XmlValueAttr<f32>,
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

impl From<YmapTimeCycleModifier> for XmlYmapTimeCycleModifier {
  fn from(v: YmapTimeCycleModifier) -> Self {
    Self {
      name: v.name,
      min_extents: v.min_extents.into(),
      max_extents: v.max_extents.into(),
      percentage: XmlValueAttr {
        value: v.percentage,
      },
      range: XmlValueAttr {
        value: v.range,
      },
      start_hour: XmlValueAttr {
        value: v.start_hour,
      },
      end_hour: XmlValueAttr {
        value: v.end_hour,
      },
    }
  }
}
