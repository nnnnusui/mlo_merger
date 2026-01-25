use serde::Deserialize;

use crate::core::format::{xml::XmlValueAttr, ymap::model::YmapDistantLodLightsSoa};

#[derive(Debug, Deserialize)]
#[serde(rename = "DistantLODLightsSOA", rename_all = "camelCase")]
pub struct XmlYmapDistantLodLightsSoa {
  #[serde(default)]
  pub position: XmlYmapStringList,
  #[serde(rename = "RGBI", default)]
  pub rgbi: XmlYmapStringList,
  pub num_street_lights: XmlValueAttr<u32>,
  pub category: XmlValueAttr<u32>,
}

impl Default for XmlYmapDistantLodLightsSoa {
  fn default() -> Self {
    Self {
      position: XmlYmapStringList::default(),
      rgbi: XmlYmapStringList::default(),
      num_street_lights: XmlValueAttr { value: 0 },
      category: XmlValueAttr { value: 0 },
    }
  }
}

#[derive(Debug, Deserialize, Default)]
pub struct XmlYmapStringList {
  #[serde(rename = "Item", default)]
  pub items: Vec<String>,
}

impl From<XmlYmapDistantLodLightsSoa> for YmapDistantLodLightsSoa {
  fn from(v: XmlYmapDistantLodLightsSoa) -> Self {
    Self {
      position: v.position.items,
      rgbi: v.rgbi.items,
      num_street_lights: v.num_street_lights.value,
      category: v.category.value,
    }
  }
}
