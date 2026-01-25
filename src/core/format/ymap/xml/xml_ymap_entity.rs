use serde::Deserialize;

use crate::core::format::{
  xml::{XmlValueAttr, position::XmlPositionAttr, rotation::XmlRotation},
  ymap::model::YmapEntity,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XmlYmapEntity {
  #[serde(rename = "@type")]
  pub entity_type: String,
  pub archetype_name: String,
  pub flags: XmlValueAttr<u32>,
  pub guid: XmlValueAttr<u32>,
  pub position: XmlPositionAttr,
  pub rotation: XmlRotation,
  pub scale_x_y: XmlValueAttr<f32>,
  pub scale_z: XmlValueAttr<f32>,
  pub parent_index: XmlValueAttr<i32>,
  pub lod_dist: XmlValueAttr<f32>,
  pub child_lod_dist: XmlValueAttr<f32>,
  pub lod_level: String,
  pub num_children: XmlValueAttr<u32>,
  pub priority_level: String,
  pub extensions: Option<()>,
  pub ambient_occlusion_multiplier: XmlValueAttr<u8>,
  pub artificial_ambient_occlusion: XmlValueAttr<u8>,
  pub tint_value: XmlValueAttr<u32>,
}

impl From<XmlYmapEntity> for YmapEntity {
  fn from(v: XmlYmapEntity) -> Self {
    Self {
      entity_type: v.entity_type,
      archetype_name: v.archetype_name,
      flags: v.flags.value,
      guid: v.guid.value,
      position: v.position.into(),
      rotation: v.rotation.into(),
      scale_x_y: v.scale_x_y.value,
      scale_z: v.scale_z.value,
      parent_index: v.parent_index.value,
      lod_dist: v.lod_dist.value,
      child_lod_dist: v.child_lod_dist.value,
      lod_level: v.lod_level,
      num_children: v.num_children.value,
      priority_level: v.priority_level,
      ambient_occlusion_multiplier: v.ambient_occlusion_multiplier.value,
      artificial_ambient_occlusion: v.artificial_ambient_occlusion.value,
      tint_value: v.tint_value.value,
    }
  }
}
