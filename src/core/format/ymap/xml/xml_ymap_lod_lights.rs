use serde::Deserialize;

use crate::core::format::ymap::model::YmapLodLightsSoa;

#[derive(Debug, Deserialize, Default)]
#[serde(rename = "LODLightsSOA", rename_all = "camelCase")]
pub struct XmlYmapLodLightsSoa {
  #[serde(default)]
  pub direction: XmlYmapStringList,
  #[serde(default)]
  pub falloff: XmlYmapStringList,
  #[serde(default)]
  pub falloff_exponent: XmlYmapStringList,
  #[serde(default)]
  pub time_and_state_flags: XmlYmapStringList,
  #[serde(default)]
  pub hash: XmlYmapStringList,
  #[serde(default)]
  pub cone_inner_angle: XmlYmapStringList,
  #[serde(default)]
  pub cone_outer_angle_or_cap_ext: XmlYmapStringList,
  #[serde(default)]
  pub corona_intensity: XmlYmapStringList,
}

#[derive(Debug, Deserialize, Default)]
pub struct XmlYmapStringList {
  #[serde(rename = "Item", default)]
  pub items: Vec<String>,
}

impl From<XmlYmapLodLightsSoa> for YmapLodLightsSoa {
  fn from(v: XmlYmapLodLightsSoa) -> Self {
    Self {
      direction: v.direction.items,
      falloff: v.falloff.items,
      falloff_exponent: v.falloff_exponent.items,
      time_and_state_flags: v.time_and_state_flags.items,
      hash: v.hash.items,
      cone_inner_angle: v.cone_inner_angle.items,
      cone_outer_angle_or_cap_ext: v.cone_outer_angle_or_cap_ext.items,
      corona_intensity: v.corona_intensity.items,
    }
  }
}
