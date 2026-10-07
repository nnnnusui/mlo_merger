use serde::{Deserialize, Serialize};

use crate::core::format::{
  xml::{XmlValueAttr, position::XmlPositionAttr, rotation::XmlRotation},
  ymap::model::YmapEntity,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XmlYmapEntity {
  #[serde(rename = "@type")]
  pub entity_type: String,
  pub archetype_name: String,
  pub flags: XmlValueAttr<u32>,
  #[serde(deserialize_with = "deserialize_guid")]
  pub guid: XmlValueAttr<u32>,
  pub position: XmlPositionAttr,
  pub rotation: XmlRotation,
  pub scale_x_y: XmlValueAttr<f32>,
  pub scale_z: XmlValueAttr<f32>,
  pub parent_index: XmlValueAttr<i32>,
  pub lod_dist: XmlValueAttr<f32>,
  /// Older embedded PSO schemas omit this field; use the zero-initialized value.
  #[serde(default)]
  pub child_lod_dist: XmlValueAttr<f32>,
  pub lod_level: String,
  pub num_children: XmlValueAttr<u32>,
  pub priority_level: String,
  pub extensions: Option<serde_json::Value>,
  pub ambient_occlusion_multiplier: XmlValueAttr<u8>,
  pub artificial_ambient_occlusion: XmlValueAttr<u8>,
  pub tint_value: XmlValueAttr<u32>,
}

/// PSO XML can spell a GUID as signed i32; preserve its original 32-bit pattern.
fn deserialize_guid<'de, D>(deserializer: D) -> Result<XmlValueAttr<u32>, D::Error>
where
  D: serde::Deserializer<'de>,
{
  let attr = XmlValueAttr::<i64>::deserialize(deserializer)?;
  let value = if attr.value < 0 {
    let signed = i32::try_from(attr.value).map_err(serde::de::Error::custom)?;
    u32::from_ne_bytes(signed.to_ne_bytes())
  } else {
    u32::try_from(attr.value).map_err(serde::de::Error::custom)?
  };
  Ok(XmlValueAttr {
    value,
  })
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

impl From<YmapEntity> for XmlYmapEntity {
  fn from(v: YmapEntity) -> Self {
    Self {
      entity_type: v.entity_type,
      archetype_name: v.archetype_name,
      flags: XmlValueAttr {
        value: v.flags,
      },
      guid: XmlValueAttr {
        value: v.guid,
      },
      position: v.position.into(),
      rotation: v.rotation.into(),
      scale_x_y: XmlValueAttr {
        value: v.scale_x_y,
      },
      scale_z: XmlValueAttr {
        value: v.scale_z,
      },
      parent_index: XmlValueAttr {
        value: v.parent_index,
      },
      lod_dist: XmlValueAttr {
        value: v.lod_dist,
      },
      child_lod_dist: XmlValueAttr {
        value: v.child_lod_dist,
      },
      lod_level: v.lod_level,
      num_children: XmlValueAttr {
        value: v.num_children,
      },
      priority_level: v.priority_level,
      extensions: None,
      ambient_occlusion_multiplier: XmlValueAttr {
        value: v.ambient_occlusion_multiplier,
      },
      artificial_ambient_occlusion: XmlValueAttr {
        value: v.artificial_ambient_occlusion,
      },
      tint_value: XmlValueAttr {
        value: v.tint_value,
      },
    }
  }
}

#[cfg(test)]
mod tests {
  use super::XmlYmapEntity;

  fn entity_xml(guid: &str) -> String {
    format!(
      r#"<Item type="CEntityDef">
        <archetypeName>cs1_railwyc_trk05</archetypeName>
        <flags value="1572872"/><guid value="{guid}"/>
        <position x="-544.0997" y="4910.33" z="89.414536"/>
        <rotation w="1" x="0" y="0" z="0"/>
        <scaleXY value="1"/><scaleZ value="1"/><parentIndex value="-1"/>
        <lodDist value="200"/><childLodDist value="0"/>
        <lodLevel>LODTYPES_DEPTH_ORPHANHD</lodLevel><numChildren value="0"/>
        <priorityLevel>PRI_REQUIRED</priorityLevel>
        <ambientOcclusionMultiplier value="255"/>
        <artificialAmbientOcclusion value="255"/><tintValue value="0"/>
      </Item>"#
    )
  }

  #[test]
  fn guid_accepts_signed_pso_and_unsigned_xml() {
    for (input, expected) in [
      ("-629962797", 3665004499),
      ("3665004499", 3665004499),
      ("-2147483648", 2147483648),
      ("-1", u32::MAX),
      ("4294967295", u32::MAX),
      ("0", 0),
    ] {
      let entity: XmlYmapEntity = quick_xml::de::from_str(&entity_xml(input)).unwrap();
      assert_eq!(entity.guid.value, expected);
      let serialized = quick_xml::se::to_string(&entity).unwrap();
      assert!(serialized.contains(&format!(r#"<guid value="{expected}""#)));
      let reparsed: XmlYmapEntity = quick_xml::de::from_str(&serialized).unwrap();
      assert_eq!(reparsed.guid.value, expected);
    }
  }

  #[test]
  fn guid_rejects_values_outside_32_bit_range() {
    for input in ["-2147483649", "4294967296", "not-a-guid"] {
      assert!(quick_xml::de::from_str::<XmlYmapEntity>(&entity_xml(input)).is_err());
    }
  }

  #[test]
  fn child_lod_dist_defaults_only_when_omitted() {
    let xml = entity_xml("0");
    let omitted = xml.replace(r#"<childLodDist value="0"/>"#, "");
    let entity: XmlYmapEntity = quick_xml::de::from_str(&omitted).unwrap();
    let model: crate::core::format::ymap::model::YmapEntity = entity.into();
    assert_eq!(model.child_lod_dist, 0.0);

    for value in ["-1", "125.5"] {
      let explicit =
        xml.replace(r#"<childLodDist value="0"/>"#, &format!(r#"<childLodDist value="{value}"/>"#));
      let entity: XmlYmapEntity = quick_xml::de::from_str(&explicit).unwrap();
      assert_eq!(entity.child_lod_dist.value, value.parse::<f32>().unwrap());
      let serialized = quick_xml::se::to_string(&entity).unwrap();
      let reparsed: XmlYmapEntity = quick_xml::de::from_str(&serialized).unwrap();
      assert_eq!(reparsed.child_lod_dist.value, entity.child_lod_dist.value);
    }

    let invalid = xml.replace(r#"<childLodDist value="0"/>"#, r#"<childLodDist value="invalid"/>"#);
    assert!(quick_xml::de::from_str::<XmlYmapEntity>(&invalid).is_err());
  }

  #[test]
  fn accepts_structured_entity_extensions() {
    let xml = entity_xml("0").replace(
      "</Item>",
      "<extensions><Item type=\"test\"><payload value=\"1\"/></Item></extensions></Item>",
    );
    let entity: XmlYmapEntity = quick_xml::de::from_str(&xml).unwrap();
    assert!(entity.extensions.is_some());
    let mut value = serde_json::to_value(&entity).unwrap();
    value["extensions"] = serde_json::json!({
      "Item": [{"@type": "test", "payload": {"@value": 1}}]
    });
    let entity: XmlYmapEntity = serde_json::from_value(value).unwrap();
    let model: crate::core::format::ymap::model::YmapEntity = entity.into();
    assert_eq!(model.guid, 0);
  }
}
