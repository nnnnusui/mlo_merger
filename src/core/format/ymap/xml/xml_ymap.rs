use serde::Deserialize;

use crate::core::format::{
  xml::{XmlValueAttr, position::XmlPositionAttr},
  ymap::{
    model::{Ymap, YmapEntity},
    xml::{
      XmlYmapBlock, XmlYmapBoxOccluder, XmlYmapDistantLodLightsSoa, XmlYmapEntity,
      XmlYmapLodLightsSoa, XmlYmapOccludeModel,
    },
  },
};

#[derive(Debug, Deserialize)]
#[serde(rename = "CMapData", rename_all = "camelCase")]
pub struct XmlYmap {
  pub name: String,
  pub parent: String,
  pub flags: XmlValueAttr<u32>,
  pub content_flags: XmlValueAttr<u32>,
  pub streaming_extents_min: XmlPositionAttr,
  pub streaming_extents_max: XmlPositionAttr,
  pub entities_extents_min: XmlPositionAttr,
  pub entities_extents_max: XmlPositionAttr,
  #[serde(default)]
  pub entities: XmlEntities,
  #[serde(default)]
  pub box_occluders: XmlBoxOccluders,
  #[serde(default)]
  pub occlude_models: XmlOccludeModels,
  #[serde(rename = "LODLightsSOA", default)]
  pub lod_lights_soa: XmlYmapLodLightsSoa,
  #[serde(rename = "DistantLODLightsSOA", default)]
  pub distant_lod_lights_soa: XmlYmapDistantLodLightsSoa,
  pub block: XmlYmapBlock,
}

impl From<XmlYmap> for Ymap {
  fn from(v: XmlYmap) -> Self {
    Self {
      name: v.name,
      parent: v.parent,
      flags: v.flags.value,
      content_flags: v.content_flags.value,
      streaming_extents_min: v.streaming_extents_min.into(),
      streaming_extents_max: v.streaming_extents_max.into(),
      entities_extents_min: v.entities_extents_min.into(),
      entities_extents_max: v.entities_extents_max.into(),
      entity_map: v
        .entities
        .items
        .into_iter()
        .map(|e| {
          let entity: YmapEntity = e.into();
          (entity.guid, entity)
        })
        .collect(),
      box_occluders: v.box_occluders.items.into_iter().map(Into::into).collect(),
      occlude_models: v.occlude_models.items.into_iter().map(Into::into).collect(),
      lod_lights_soa: v.lod_lights_soa.into(),
      distant_lod_lights_soa: v.distant_lod_lights_soa.into(),
      block: v.block.into(),
    }
  }
}

#[derive(Debug, Deserialize, Default)]
pub struct XmlEntities {
  #[serde(rename = "Item", default)]
  pub items: Vec<XmlYmapEntity>,
}

#[derive(Debug, Deserialize, Default)]
pub struct XmlBoxOccluders {
  #[serde(rename = "Item", default)]
  pub items: Vec<XmlYmapBoxOccluder>,
}

#[derive(Debug, Deserialize, Default)]
pub struct XmlOccludeModels {
  #[serde(rename = "Item", default)]
  pub items: Vec<XmlYmapOccludeModel>,
}
