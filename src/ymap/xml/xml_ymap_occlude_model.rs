use serde::Deserialize;

use crate::{
  xml::{XmlValueAttr, position::XmlPositionAttr},
  ymap::model::YmapOccludeModel,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XmlYmapOccludeModel {
  pub bmin: XmlPositionAttr,
  pub bmax: XmlPositionAttr,
  pub data_size: XmlValueAttr<u32>,
  pub verts: XmlYmapVertsAttr,
  pub num_verts_in_bytes: XmlValueAttr<u32>,
  pub num_tris: XmlValueAttr<u32>,
  pub flags: XmlValueAttr<u32>,
}

#[derive(Debug, Deserialize)]
pub struct XmlYmapVertsAttr {
  #[serde(rename = "@content")]
  pub content: String,
  #[serde(rename = "$value", default)]
  pub value: String,
}

impl From<XmlYmapOccludeModel> for YmapOccludeModel {
  fn from(v: XmlYmapOccludeModel) -> Self {
    // Parse hex bytes from the verts value
    let verts = parse_hex_bytes(&v.verts.value);

    Self {
      bmin: v.bmin.into(),
      bmax: v.bmax.into(),
      data_size: v.data_size.value,
      verts,
      num_verts_in_bytes: v.num_verts_in_bytes.value,
      num_tris: v.num_tris.value,
      flags: v.flags.value,
    }
  }
}

/// Parse hex string like "0xE0 0xF2 0x9A ..." into Vec<u8>
fn parse_hex_bytes(s: &str) -> Vec<u8> {
  s.split_whitespace()
    .filter(|part| part.starts_with("0x"))
    .filter_map(|hex| u8::from_str_radix(&hex[2..], 16).ok())
    .collect()
}
