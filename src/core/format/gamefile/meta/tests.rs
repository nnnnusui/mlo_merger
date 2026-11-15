use crate::core::format::gamefile::{
  rsc7::Rsc7Resource,
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

/// Ped variation YMTs carry 16-bit enums with named values and flags whose enums the file
/// does not embed; both round-trip through XML.
#[test]
fn xml_rebuild_reads_named_short_enums_and_empty_unavailable_flags() {
  use super::xml::meta_from_xml;
  use super::{
    MetaEnumEntry, MetaEnumInfo, MetaSchemaCatalog, MetaStructureEntry, MetaStructureInfo,
  };
  use crate::core::format::gamefile::meta::jenk_hash;

  let entry =
    |name: &str, data_type: u8, data_offset: u32, reference_key: u32| MetaStructureEntry {
      name_hash: jenk_hash(name),
      data_offset,
      data_type,
      unknown: 0,
      reference_type_index: 0,
      reference_key,
    };
  let mut catalog = MetaSchemaCatalog::default();
  catalog.structures.insert(
    jenk_hash("CTestInfo"),
    MetaStructureInfo {
      name_hash: jenk_hash("CTestInfo"),
      structure_key: 1,
      unknown_8: 0,
      unknown_12: 0,
      unknown_28: 0,
      structure_size: 8,
      entries: vec![
        entry("kind", 0x64, 0, jenk_hash("ETestKind")),
        // The flags enum is not embedded, as for ped-variation inclusions/exclusions.
        entry("inclusions", 0x63, 4, 0),
      ],
    },
  );
  catalog.enums.insert(
    jenk_hash("ETestKind"),
    MetaEnumInfo {
      name_hash: jenk_hash("ETestKind"),
      enum_key: 1,
      unknown_20: 0,
      entries: vec![MetaEnumEntry {
        name_hash: 0x451A_F524,
        value: 3,
      }],
    },
  );
  let meta =
    meta_from_xml("<CTestInfo><kind>hash_451AF524</kind><inclusions /></CTestInfo>", &catalog)
      .unwrap();
  let root = &meta.data_blocks[meta.root_block_index as usize - 1];
  assert_eq!(i16::from_le_bytes(root.data[0..2].try_into().unwrap()), 3);
  assert_eq!(i32::from_le_bytes(root.data[4..8].try_into().unwrap()), 0);
  // A named value of a missing flags enum still fails rather than dropping flags.
  let error =
    meta_from_xml("<CTestInfo><kind>0</kind><inclusions>FLAG_A</inclusions></CTestInfo>", &catalog)
      .unwrap_err();
  assert!(error.to_string().contains("enum 00000000"), "{error}");
}
