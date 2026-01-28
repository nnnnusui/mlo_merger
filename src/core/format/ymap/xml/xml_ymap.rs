use serde::{Deserialize, Serialize};

use crate::core::format::{
  xml::{XmlValueAttr, position::XmlPositionAttr},
  ymap::{
    model::{Ymap, YmapEntity},
    xml::{
      XmlYmapBlock, XmlYmapBoxOccluder, XmlYmapCarGenerator, XmlYmapDistantLodLightsSoa,
      XmlYmapEntity, XmlYmapInstancedData, XmlYmapLodLightsSoa, XmlYmapOccludeModel,
      XmlYmapTimeCycleModifier,
    },
  },
};

#[derive(Debug, Deserialize, Serialize)]
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
  pub container_lods: XmlContainerLods,
  #[serde(default)]
  pub box_occluders: XmlBoxOccluders,
  #[serde(default)]
  pub occlude_models: XmlOccludeModels,
  #[serde(default)]
  pub physics_dictionaries: XmlPhysicsDictionaries,
  #[serde(default)]
  pub instanced_data: XmlYmapInstancedData,
  #[serde(default)]
  pub time_cycle_modifiers: XmlTimeCycleModifiers,
  #[serde(default)]
  pub car_generators: XmlCarGenerators,
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
      container_lods: v.container_lods.items,
      box_occluders: v.box_occluders.items.into_iter().map(Into::into).collect(),
      occlude_models: v.occlude_models.items.into_iter().map(Into::into).collect(),
      physics_dictionaries: v.physics_dictionaries.items,
      instanced_data: v.instanced_data.into(),
      time_cycle_modifiers: v.time_cycle_modifiers.items.into_iter().map(Into::into).collect(),
      car_generators: v.car_generators.items.into_iter().map(Into::into).collect(),
      lod_lights_soa: v.lod_lights_soa.into(),
      distant_lod_lights_soa: v.distant_lod_lights_soa.into(),
      block: v.block.into(),
    }
  }
}

impl From<Ymap> for XmlYmap {
  fn from(v: Ymap) -> Self {
    Self {
      name: v.name,
      parent: v.parent,
      flags: XmlValueAttr {
        value: v.flags,
      },
      content_flags: XmlValueAttr {
        value: v.content_flags,
      },
      streaming_extents_min: v.streaming_extents_min.into(),
      streaming_extents_max: v.streaming_extents_max.into(),
      entities_extents_min: v.entities_extents_min.into(),
      entities_extents_max: v.entities_extents_max.into(),
      entities: XmlEntities {
        items: v.entity_map.into_values().map(XmlYmapEntity::from).collect(),
      },
      container_lods: XmlContainerLods {
        items: v.container_lods,
      },
      box_occluders: XmlBoxOccluders {
        items: v.box_occluders.into_iter().map(XmlYmapBoxOccluder::from).collect(),
      },
      occlude_models: XmlOccludeModels {
        items: v.occlude_models.into_iter().map(XmlYmapOccludeModel::from).collect(),
      },
      physics_dictionaries: XmlPhysicsDictionaries {
        items: v.physics_dictionaries,
      },
      instanced_data: v.instanced_data.into(),
      time_cycle_modifiers: XmlTimeCycleModifiers {
        items: v.time_cycle_modifiers.into_iter().map(XmlYmapTimeCycleModifier::from).collect(),
      },
      car_generators: XmlCarGenerators {
        items: v.car_generators.into_iter().map(XmlYmapCarGenerator::from).collect(),
      },
      lod_lights_soa: v.lod_lights_soa.into(),
      distant_lod_lights_soa: v.distant_lod_lights_soa.into(),
      block: v.block.into(),
    }
  }
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlEntities {
  #[serde(rename = "Item", default)]
  pub items: Vec<XmlYmapEntity>,
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlBoxOccluders {
  #[serde(rename = "Item", default)]
  pub items: Vec<XmlYmapBoxOccluder>,
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlOccludeModels {
  #[serde(rename = "Item", default)]
  pub items: Vec<XmlYmapOccludeModel>,
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlContainerLods {
  #[serde(rename = "Item", default)]
  pub items: Vec<String>,
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlPhysicsDictionaries {
  #[serde(rename = "Item", default)]
  pub items: Vec<String>,
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlTimeCycleModifiers {
  #[serde(rename = "Item", default)]
  pub items: Vec<XmlYmapTimeCycleModifier>,
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlCarGenerators {
  #[serde(rename = "Item", default)]
  pub items: Vec<XmlYmapCarGenerator>,
}
