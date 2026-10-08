use std::collections::HashMap;

use super::{meta_to_xml, ymap_to_xml};
use crate::core::format::gamefile::{
  meta_resource::{MetaDataBlock, MetaResource, MetaStructureEntry, MetaStructureInfo},
  resource_file::Rsc7Resource,
  test_support::{sample_ymap_binary, sample_ymap_xml},
};

#[test]
fn decodes_ymap_binary_directly_into_the_typed_model() {
  let entry = |name: &str, offset, data_type, reference_key| MetaStructureEntry {
    name_hash: super::jenk_hash(name),
    data_offset: offset,
    data_type,
    unknown: 0,
    reference_type_index: 0,
    reference_key,
  };
  let block_hash = super::jenk_hash("CMapDataBlock");
  let root_hash = super::jenk_hash("CMapData");
  let structures = vec![
    MetaStructureInfo {
      name_hash: root_hash,
      structure_key: 0,
      unknown_8: 0,
      unknown_12: 0,
      unknown_28: 0,
      structure_size: 104,
      entries: vec![
        entry("name", 0, 0x4a, 0),
        entry("parent", 4, 0x4a, 0),
        entry("flags", 8, 0x15, 0),
        entry("contentFlags", 12, 0x15, 0),
        entry("streamingExtentsMin", 16, 0x33, 0),
        entry("streamingExtentsMax", 28, 0x33, 0),
        entry("entitiesExtentsMin", 40, 0x33, 0),
        entry("entitiesExtentsMax", 52, 0x33, 0),
        MetaStructureEntry {
          name_hash: 0x100,
          data_offset: 0,
          data_type: 0x05,
          unknown: 0,
          reference_type_index: 0,
          reference_key: super::jenk_hash("CEntityDef"),
        },
        entry("entities", 64, 0x52, 0),
        entry("block", 80, 0x05, block_hash),
      ],
    },
    MetaStructureInfo {
      name_hash: block_hash,
      structure_key: 0,
      unknown_8: 0,
      unknown_12: 0,
      unknown_28: 0,
      structure_size: 24,
      entries: vec![
        entry("version", 0, 0x15, 0),
        entry("flags", 4, 0x15, 0),
        entry("name", 8, 0x4a, 0),
        entry("exportedBy", 12, 0x4a, 0),
        entry("owner", 16, 0x4a, 0),
        entry("time", 20, 0x4a, 0),
      ],
    },
  ];
  let mut data = vec![0; 104];
  data[8..12].copy_from_slice(&7u32.to_le_bytes());
  data[12..16].copy_from_slice(&11u32.to_le_bytes());
  let meta = MetaResource {
    root_block_index: 1,
    structures,
    enums: vec![],
    data_blocks: vec![MetaDataBlock {
      structure_name_hash: root_hash,
      data,
    }],
    name: Some("fixture".into()),
  };
  let bytes = meta.to_rsc7(2).unwrap().encode().unwrap();

  let model = super::ymap_to_model(&bytes, &HashMap::new()).unwrap();
  assert_eq!(model.name, "");
  assert_eq!(model.flags, 7);
  assert_eq!(model.content_flags, 11);
  assert!(model.entity_map.is_empty());
  assert_eq!(model.block.name, "");
  let xml = super::ymap_to_xml(&bytes, &HashMap::new()).unwrap();
  let xml_model: crate::core::format::ymap::model::Ymap =
    quick_xml::de::from_str::<crate::core::format::ymap::xml::XmlYmap>(&xml).unwrap().into();
  assert_eq!(model, xml_model);
}

#[test]
fn serializes_sample_occlusion_ymap_using_embedded_schema() {
  let xml = sample_ymap_xml("occlusion/occlusion.ymap.xml");
  let bytes = sample_ymap_binary(&xml);
  let xml = ymap_to_xml(&bytes, &HashMap::new()).unwrap();

  assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapData>"));
  assert!(xml.contains("<boxOccluders itemType=\"BoxOccluder\">"));
  assert!(xml.contains("<iCenterX value=\"-1567\" />"));
  assert!(xml.contains("<occludeModels"));
}

#[test]
fn lod_light_hash_array_uses_its_canonical_name() {
  let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("asset/source/isacb_mechanic_littlesoul/stream/metadata/mek_lodlights_lodlights.ymap");
  let bytes = std::fs::read(fixture).unwrap();
  let resource = Rsc7Resource::decode(&bytes).unwrap();
  let meta = MetaResource::parse(&resource).unwrap();
  let xml = meta_to_xml(&meta, &HashMap::new()).unwrap();
  assert!(xml.contains("<hash>"));
  assert!(!xml.contains("hash_0000004A"));
  let parsed: crate::core::format::ymap::xml::XmlYmap = quick_xml::de::from_str(&xml).unwrap();
  let model: crate::core::format::ymap::model::Ymap = parsed.into();
  assert_eq!(model.lod_lights.len(), 184);
  assert_eq!(model.lod_lights[0].hash, "1571135");
}

#[test]
fn serializes_ytyp_rsc_meta_using_the_shared_meta_writer() {
  let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("asset/source/sb_trainheistmap/stream/sb_train_addonprops.ytyp");
  let bytes = std::fs::read(fixture).unwrap();
  let resource = Rsc7Resource::decode(&bytes).unwrap();
  let meta = MetaResource::parse(&resource).unwrap();
  let xml = meta_to_xml(&meta, &meta.hash_names()).unwrap();

  assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
  assert!(xml.contains("<CMapTypes"));
}

#[test]
fn serializes_all_checked_in_ymap_resources() {
  let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/extracted");
  let mut converted = 0;

  for entry in std::fs::read_dir(fixtures).unwrap() {
    let path = entry.unwrap().path();
    if path.extension().is_some_and(|extension| extension == "ymap") {
      let bytes = std::fs::read(&path).unwrap();
      ymap_to_xml(&bytes, &HashMap::new()).unwrap_or_else(|error| {
        panic!("failed to serialize {}: {error}", path.display());
      });
      converted += 1;
    }
  }

  assert!(converted > 0);
}
