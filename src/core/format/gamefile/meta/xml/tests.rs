use std::collections::HashMap;

use crate::core::format::gamefile::{
  meta::{MetaDataBlock, MetaResource, MetaStructureEntry, MetaStructureInfo},
  rsc7::Rsc7Resource,
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

  let model = crate::core::format::ymap::binary::ymap_to_model(&bytes, &HashMap::new()).unwrap();
  assert_eq!(model.name, "");
  assert_eq!(model.flags, 7);
  assert_eq!(model.content_flags, 11);
  assert!(model.entities.is_empty());
  assert_eq!(model.block.name, "");
  // The XML export rebuilds the same model.
  let parsed = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
  let xml = parsed.to_xml(&HashMap::new()).unwrap();
  let mut catalog = crate::core::format::gamefile::meta::MetaSchemaCatalog::default();
  catalog.add_resource(&parsed);
  let rebuilt = crate::core::format::stream_file::StreamFile::encode_xml(
    crate::core::format::stream_file::FileType::Ymap,
    &xml,
    &catalog,
  )
  .unwrap();
  let xml_model =
    crate::core::format::ymap::binary::ymap_to_model(&rebuilt, &HashMap::new()).unwrap();
  assert_eq!(model, xml_model);
}

#[test]
fn serializes_sample_occlusion_ymap_using_embedded_schema() {
  let xml = sample_ymap_xml("occlusion/occlusion.ymap.xml");
  let bytes = sample_ymap_binary(&xml);
  let xml = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap())
    .unwrap()
    .to_xml(&HashMap::new())
    .unwrap();

  assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<CMapData>"));
  assert!(xml.contains("<boxOccluders itemType=\"BoxOccluder\">"));
  assert!(xml.contains("<iCenterX value=\"-1567\" />"));
  assert!(xml.contains("<occludeModels"));
}

#[test]
fn serializes_ytyp_rsc_meta_using_the_shared_meta_writer() {
  let root_hash = super::jenk_hash("CMapTypes");
  let name_hash = super::jenk_hash("fixture_types");
  let meta = MetaResource {
    root_block_index: 1,
    structures: vec![MetaStructureInfo {
      name_hash: root_hash,
      structure_key: 0,
      unknown_8: 0,
      unknown_12: 0,
      unknown_28: 0,
      structure_size: 4,
      entries: vec![MetaStructureEntry {
        name_hash: super::jenk_hash("name"),
        data_offset: 0,
        data_type: 0x4a,
        unknown: 0,
        reference_type_index: 0,
        reference_key: 0,
      }],
    }],
    enums: vec![],
    data_blocks: vec![MetaDataBlock {
      structure_name_hash: root_hash,
      data: name_hash.to_le_bytes().to_vec(),
    }],
    name: Some("fixture_types".into()),
  };
  let bytes = meta.to_rsc7(2).unwrap().encode().unwrap();
  let resource = Rsc7Resource::decode(&bytes).unwrap();
  let meta = MetaResource::parse(&resource).unwrap();
  let names = HashMap::from([(name_hash, "fixture_types".to_string())]);
  let xml = meta.to_xml(&names).unwrap();

  assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
  assert!(xml.contains("<CMapTypes"));
  assert!(xml.contains("<name>fixture_types</name>"));
}
