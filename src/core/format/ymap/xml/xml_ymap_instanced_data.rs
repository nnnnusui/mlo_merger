use serde::{Deserialize, Serialize};

use crate::core::format::{
  xml::XmlValueAttr,
  ymap::model::{
    BoundingBox, GrassInstance, GrassInstanceBatch, Vector3, Vector4, YmapInstancedData,
  },
};

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlYmapInstancedData {
  #[serde(default)]
  pub error: Option<String>,
  #[serde(rename = "ImapLink", default)]
  pub imap_link: String,
  #[serde(rename = "PropInstanceList", default)]
  pub prop_instance_list: XmlPropInstanceList,
  #[serde(rename = "GrassInstanceList", default)]
  pub grass_instance_list: XmlGrassInstanceList,
}

impl From<XmlYmapInstancedData> for YmapInstancedData {
  fn from(v: XmlYmapInstancedData) -> Self {
    // If there's an error, log it and return empty data
    if let Some(error) = v.error {
      log::warn!("InstancedData error: {}", error);
      return Self::default();
    }

    // Convert grass instance list to structured data
    let grass_instance_list =
      v.grass_instance_list.items.into_iter().map(GrassInstanceBatch::from).collect();

    Self {
      imap_link: v.imap_link,
      prop_instance_list: v.prop_instance_list.items,
      grass_instance_list,
    }
  }
}

impl From<YmapInstancedData> for XmlYmapInstancedData {
  fn from(v: YmapInstancedData) -> Self {
    Self {
      error: None,
      imap_link: v.imap_link,
      prop_instance_list: v.prop_instance_list.into(),
      grass_instance_list: XmlGrassInstanceList {
        items: v.grass_instance_list.into_iter().map(XmlGrassInstanceItem::from).collect(),
      },
    }
  }
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlPropInstanceList {
  #[serde(rename = "Item", default)]
  pub items: Vec<String>,
}

impl From<Vec<String>> for XmlPropInstanceList {
  fn from(items: Vec<String>) -> Self {
    Self {
      items,
    }
  }
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlGrassInstanceList {
  #[serde(rename = "$value", default)]
  pub items: Vec<XmlGrassInstanceItem>,
}

#[derive(Debug, Deserialize, Default, Serialize)]
#[serde(rename = "Item")]
pub struct XmlGrassInstanceItem {
  #[serde(rename = "BatchAABB", default)]
  pub batch_aabb: XmlBatchAABB,
  #[serde(rename = "ScaleRange", default)]
  pub scale_range: XmlVector3,
  #[serde(rename = "archetypeName", default)]
  pub archetype_name: String,
  #[serde(rename = "lodDist", default)]
  pub lod_dist: XmlValueAttr<f32>,
  #[serde(rename = "LodFadeStartDist", default)]
  pub lod_fade_start_dist: XmlValueAttr<f32>,
  #[serde(rename = "LodInstFadeRange", default)]
  pub lod_inst_fade_range: XmlValueAttr<f32>,
  #[serde(rename = "OrientToTerrain", default)]
  pub orient_to_terrain: XmlValueAttr<u32>,
  #[serde(rename = "InstanceList", default)]
  pub instance_list: XmlGrassInstanceDataList,
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlBatchAABB {
  #[serde(default)]
  pub min: XmlVector4,
  #[serde(default)]
  pub max: XmlVector4,
}

impl From<XmlBatchAABB> for BoundingBox {
  fn from(v: XmlBatchAABB) -> Self {
    Self {
      min: v.min.into(),
      max: v.max.into(),
    }
  }
}

impl From<BoundingBox> for XmlBatchAABB {
  fn from(v: BoundingBox) -> Self {
    Self {
      min: v.min.into(),
      max: v.max.into(),
    }
  }
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlVector3 {
  #[serde(rename = "@x", default)]
  pub x: f32,
  #[serde(rename = "@y", default)]
  pub y: f32,
  #[serde(rename = "@z", default)]
  pub z: f32,
}

impl From<XmlVector3> for Vector3 {
  fn from(v: XmlVector3) -> Self {
    Self {
      x: v.x,
      y: v.y,
      z: v.z,
    }
  }
}

impl From<Vector3> for XmlVector3 {
  fn from(v: Vector3) -> Self {
    Self {
      x: v.x,
      y: v.y,
      z: v.z,
    }
  }
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlVector4 {
  #[serde(rename = "@x", default)]
  pub x: f32,
  #[serde(rename = "@y", default)]
  pub y: f32,
  #[serde(rename = "@z", default)]
  pub z: f32,
  #[serde(rename = "@w", default)]
  pub w: f32,
}

impl From<XmlVector4> for Vector4 {
  fn from(v: XmlVector4) -> Self {
    Self {
      x: v.x,
      y: v.y,
      z: v.z,
      w: v.w,
    }
  }
}

impl From<Vector4> for XmlVector4 {
  fn from(v: Vector4) -> Self {
    Self {
      x: v.x,
      y: v.y,
      z: v.z,
      w: v.w,
    }
  }
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlGrassInstanceDataList {
  #[serde(rename = "$value", default)]
  pub items: Vec<XmlGrassInstanceData>,
}

#[derive(Debug, Deserialize, Default, Serialize)]
#[serde(rename = "Item")]
pub struct XmlGrassInstanceData {
  #[serde(rename = "Position", default)]
  pub position: XmlNumberList,
  #[serde(rename = "NormalX", default)]
  pub normal_x: XmlValueAttr<u32>,
  #[serde(rename = "NormalY", default)]
  pub normal_y: XmlValueAttr<u32>,
  #[serde(rename = "Color", default)]
  pub color: XmlNumberList,
  #[serde(rename = "Scale", default)]
  pub scale: XmlValueAttr<u32>,
  #[serde(rename = "Ao", default)]
  pub ao: XmlValueAttr<u32>,
  #[serde(rename = "Pad", default)]
  pub pad: XmlNumberList,
}

#[derive(Debug, Deserialize, Default, Serialize)]
pub struct XmlNumberList {
  #[serde(rename = "$value", default)]
  pub text: String,
}

impl From<XmlGrassInstanceItem> for GrassInstanceBatch {
  fn from(v: XmlGrassInstanceItem) -> Self {
    Self {
      batch_aabb: v.batch_aabb.into(),
      scale_range: v.scale_range.into(),
      archetype_name: v.archetype_name,
      lod_dist: v.lod_dist.value,
      lod_fade_start_dist: v.lod_fade_start_dist.value,
      lod_inst_fade_range: v.lod_inst_fade_range.value,
      orient_to_terrain: v.orient_to_terrain.value,
      instances: v.instance_list.items.into_iter().map(GrassInstance::from).collect(),
    }
  }
}

impl From<GrassInstanceBatch> for XmlGrassInstanceItem {
  fn from(v: GrassInstanceBatch) -> Self {
    Self {
      batch_aabb: v.batch_aabb.into(),
      scale_range: v.scale_range.into(),
      archetype_name: v.archetype_name,
      lod_dist: XmlValueAttr {
        value: v.lod_dist,
      },
      lod_fade_start_dist: XmlValueAttr {
        value: v.lod_fade_start_dist,
      },
      lod_inst_fade_range: XmlValueAttr {
        value: v.lod_inst_fade_range,
      },
      orient_to_terrain: XmlValueAttr {
        value: v.orient_to_terrain,
      },
      instance_list: XmlGrassInstanceDataList {
        items: v.instances.into_iter().map(XmlGrassInstanceData::from).collect(),
      },
    }
  }
}

impl From<XmlGrassInstanceData> for GrassInstance {
  fn from(v: XmlGrassInstanceData) -> Self {
    // Parse space-separated numbers from text
    let parse_numbers =
      |text: &str| -> Vec<f32> { text.split_whitespace().filter_map(|s| s.parse().ok()).collect() };

    let parse_u32_numbers =
      |text: &str| -> Vec<u32> { text.split_whitespace().filter_map(|s| s.parse().ok()).collect() };

    Self {
      position: parse_numbers(&v.position.text),
      normal_x: v.normal_x.value,
      normal_y: v.normal_y.value,
      color: parse_u32_numbers(&v.color.text),
      scale: v.scale.value,
      ao: v.ao.value,
      pad: parse_u32_numbers(&v.pad.text),
    }
  }
}

impl From<GrassInstance> for XmlGrassInstanceData {
  fn from(v: GrassInstance) -> Self {
    Self {
      position: XmlNumberList {
        text: v.position.iter().map(|n| n.to_string()).collect::<Vec<String>>().join(" "),
      },
      normal_x: XmlValueAttr {
        value: v.normal_x,
      },
      normal_y: XmlValueAttr {
        value: v.normal_y,
      },
      color: XmlNumberList {
        text: v.color.iter().map(|n| n.to_string()).collect::<Vec<String>>().join(" "),
      },
      scale: XmlValueAttr {
        value: v.scale,
      },
      ao: XmlValueAttr {
        value: v.ao,
      },
      pad: XmlNumberList {
        text: v.pad.iter().map(|n| n.to_string()).collect::<Vec<String>>().join(" "),
      },
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_instanced_data_with_error() {
    let xml = r#"
      <instancedData>
        <error>Couldn't find structure info!</error>
      </instancedData>
    "#;

    let parsed: XmlYmapInstancedData = quick_xml::de::from_str(xml).unwrap();
    assert!(parsed.error.is_some());

    let converted: YmapInstancedData = parsed.into();
    assert_eq!(converted.imap_link, "");
    assert_eq!(converted.prop_instance_list.len(), 0);
    assert_eq!(converted.grass_instance_list.len(), 0);
  }

  #[test]
  fn test_parse_instanced_data_with_grass() {
    let xml = r#"
      <instancedData>
        <ImapLink />
        <PropInstanceList itemType="rage__fwPropInstanceListDef" />
        <GrassInstanceList itemType="rage__fwGrassInstanceListDef">
          <Item>
            <BatchAABB>
              <min x="1083.04724" y="-829.376953" z="51.5762024" w="0" />
              <max x="1174.319" y="-783.113159" z="57.9404831" w="0" />
            </BatchAABB>
            <ScaleRange x="0.6" y="1.2" z="0.3" />
            <archetypeName>urbangrngrass_01</archetypeName>
            <lodDist value="50" />
            <LodFadeStartDist value="15" />
            <LodInstFadeRange value="0.75" />
            <OrientToTerrain value="1" />
            <InstanceList itemType="rage__fwGrassInstanceListDef__InstanceData" />
          </Item>
        </GrassInstanceList>
      </instancedData>
    "#;

    let parsed: XmlYmapInstancedData = quick_xml::de::from_str(xml).unwrap();
    assert!(parsed.error.is_none());
    assert_eq!(parsed.grass_instance_list.items.len(), 1);
    assert_eq!(parsed.grass_instance_list.items[0].archetype_name, "urbangrngrass_01");

    let converted: YmapInstancedData = parsed.into();
    assert_eq!(converted.grass_instance_list.len(), 1);
    assert_eq!(converted.grass_instance_list[0].archetype_name, "urbangrngrass_01");
    assert_eq!(converted.grass_instance_list[0].lod_dist, 50.0);
    assert_eq!(converted.grass_instance_list[0].scale_range.x, 0.6);
  }

  #[test]
  fn test_parse_empty_instanced_data() {
    let xml = r#"
      <instancedData>
        <ImapLink />
        <PropInstanceList itemType="rage__fwPropInstanceListDef" />
        <GrassInstanceList itemType="rage__fwGrassInstanceListDef" />
      </instancedData>
    "#;

    let parsed: XmlYmapInstancedData = quick_xml::de::from_str(xml).unwrap();
    assert!(parsed.error.is_none());

    let converted: YmapInstancedData = parsed.into();
    assert_eq!(converted.grass_instance_list.len(), 0);
  }
}
