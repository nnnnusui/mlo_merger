use serde::Deserialize;

use crate::core::format::ymap::model::YmapInstancedData;

#[derive(Debug, Deserialize, Default)]
pub struct XmlYmapInstancedData {
  #[serde(rename = "ImapLink", default)]
  pub imap_link: String,
  #[serde(rename = "PropInstanceList", default)]
  pub prop_instance_list: XmlInstanceList,
  #[serde(rename = "GrassInstanceList", default)]
  pub grass_instance_list: XmlInstanceList,
}

#[derive(Debug, Deserialize, Default)]
pub struct XmlInstanceList {
  #[serde(rename = "Item", default)]
  pub items: Vec<String>,
}

impl From<XmlYmapInstancedData> for YmapInstancedData {
  fn from(v: XmlYmapInstancedData) -> Self {
    Self {
      imap_link: v.imap_link,
      prop_instance_list: v.prop_instance_list.items,
      grass_instance_list: v.grass_instance_list.items,
    }
  }
}
