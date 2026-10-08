use crate::core::format::gamefile::{
  resource_file::Rsc7Resource,
  test_support::{sample_ymap_binary, sample_ymap_xml},
};

use super::MetaResource;

#[test]
fn schema_keys_and_reserved_metadata_survive_rebuild() {
  let xml = sample_ymap_xml("parent_refs/child.ymap.xml");
  let resource = Rsc7Resource::decode(&sample_ymap_binary(&xml)).unwrap();
  let mut meta = MetaResource::parse(&resource).unwrap();
  meta.structures[0].structure_key = 0x1234_5678;
  meta.structures[0].unknown_8 = 0x400;
  meta.structures[0].unknown_12 = 0x1122_3344;
  meta.structures[0].unknown_28 = 7;
  meta.enums.push(super::MetaEnumInfo {
    name_hash: 0x1020_3040,
    enum_key: 0x5566_7788,
    unknown_20: 9,
    entries: vec![super::MetaEnumEntry {
      name_hash: 0x5060_7080,
      value: 3,
    }],
  });
  let rebuilt = meta.to_rsc7(resource.version).unwrap();
  assert_eq!(MetaResource::parse(&rebuilt).unwrap(), meta);
}

#[test]
fn parses_embedded_meta_tables_from_ymap() {
  let xml = sample_ymap_xml("parent_refs/child.ymap.xml");
  let bytes = sample_ymap_binary(&xml);
  let resource = Rsc7Resource::decode(&bytes).unwrap();
  let meta = MetaResource::parse(&resource).unwrap();

  assert!(meta.root_block_index > 0);
  assert!(!meta.structures.is_empty());
  assert!(!meta.data_blocks.is_empty());
  let root = &meta.data_blocks[meta.root_block_index as usize - 1];
  assert!(meta.structures.iter().any(|s| s.name_hash == root.structure_name_hash));
}

#[test]
fn rebuilds_embedded_meta_tables_in_rsc7() {
  let xml = sample_ymap_xml("parent_refs/child.ymap.xml");
  let bytes = sample_ymap_binary(&xml);
  let original = Rsc7Resource::decode(&bytes).unwrap();
  let meta = MetaResource::parse(&original).unwrap();

  let rebuilt = meta.to_rsc7(original.version).unwrap();
  let encoded = rebuilt.encode().unwrap();
  let decoded = Rsc7Resource::decode(&encoded).unwrap();
  let reparsed = MetaResource::parse(&decoded).unwrap();

  assert_eq!(reparsed.root_block_index, meta.root_block_index);
  assert_eq!(reparsed.structures, meta.structures);
  assert_eq!(reparsed.enums, meta.enums);
  assert_eq!(reparsed.data_blocks.len(), meta.data_blocks.len());
  for (index, (actual, expected)) in reparsed.data_blocks.iter().zip(&meta.data_blocks).enumerate()
  {
    assert_eq!(
      actual.structure_name_hash, expected.structure_name_hash,
      "block {index} type differs"
    );
    assert_eq!(actual.data.len(), expected.data.len(), "block {index} length differs");
    if let Some(offset) = actual.data.iter().zip(&expected.data).position(|(a, b)| a != b) {
      panic!(
        "block {index} differs at byte {offset}: rebuilt={:02X}, original={:02X}",
        actual.data[offset], expected.data[offset]
      );
    }
  }
}

#[test]
fn parses_meta_tables_from_all_checked_in_ymaps() {
  let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/extracted");
  let mut parsed = 0;

  for entry in std::fs::read_dir(fixtures).unwrap() {
    let path = entry.unwrap().path();
    if path.extension().is_some_and(|extension| extension == "ymap") {
      let bytes = std::fs::read(&path).unwrap();
      let resource = Rsc7Resource::decode(&bytes).unwrap();
      MetaResource::parse(&resource).unwrap_or_else(|error| {
        panic!("failed to parse META in {}: {error}", path.display());
      });
      parsed += 1;
    }
  }

  assert!(parsed > 0);
}
