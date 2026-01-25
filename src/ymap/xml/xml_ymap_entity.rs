use serde::Deserialize;

use crate::{
  xml::{position::XmlPositionAttr, rotation::XmlRotation},
  ymap::model::YmapEntity,
};

#[derive(Debug, Deserialize, Default)]
pub struct XmlEntities {
  #[serde(rename = "Item", default)]
  pub items: Vec<XmlEntity>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XmlEntity {
  #[serde(rename = "@type")]
  pub entity_type: String,
  pub archetype_name: String,
  pub flags: EntityFlags,
  pub guid: EntityGuid,
  pub position: XmlPositionAttr,
  pub rotation: XmlRotation,
  pub scale_x_y: ScaleValue,
  pub scale_z: ScaleValue,
  pub parent_index: ParentIndex,
  pub lod_dist: LodDist,
  pub child_lod_dist: ChildLodDist,
  pub lod_level: String,
  pub num_children: NumChildren,
  pub priority_level: String,
  pub extensions: Option<()>,
  pub ambient_occlusion_multiplier: AmbientOcclusionMultiplier,
  pub artificial_ambient_occlusion: ArtificialAmbientOcclusion,
  pub tint_value: TintValue,
}

impl From<XmlEntity> for YmapEntity {
  fn from(v: XmlEntity) -> Self {
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

#[derive(Debug, Deserialize)]
pub struct EntityFlags {
  #[serde(rename = "@value")]
  pub value: u32,
}

#[derive(Debug, Deserialize)]
pub struct EntityGuid {
  #[serde(rename = "@value")]
  pub value: u32,
}

#[derive(Debug, Deserialize)]
pub struct ScaleValue {
  #[serde(rename = "@value")]
  pub value: f32,
}

#[derive(Debug, Deserialize)]
pub struct ParentIndex {
  #[serde(rename = "@value")]
  pub value: i32,
}

#[derive(Debug, Deserialize)]
pub struct LodDist {
  #[serde(rename = "@value")]
  pub value: f32,
}

#[derive(Debug, Deserialize)]
pub struct ChildLodDist {
  #[serde(rename = "@value")]
  pub value: f32,
}

#[derive(Debug, Deserialize)]
pub struct NumChildren {
  #[serde(rename = "@value")]
  pub value: u32,
}

#[derive(Debug, Deserialize)]
pub struct AmbientOcclusionMultiplier {
  #[serde(rename = "@value")]
  pub value: u8,
}

#[derive(Debug, Deserialize)]
pub struct ArtificialAmbientOcclusion {
  #[serde(rename = "@value")]
  pub value: u8,
}

#[derive(Debug, Deserialize)]
pub struct TintValue {
  #[serde(rename = "@value")]
  pub value: u32,
}
