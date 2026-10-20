use super::super::model::Ymap;
use super::{meta::Writer, values::primitive, write_ymap};
use crate::core::format::gamefile::meta::{MetaResource, MetaSchemaCatalog, jenk_hash};
use crate::core::format::gamefile::meta::{MetaStructureEntry, MetaStructureInfo};
#[test]
fn typed_structure_writes_numeric_fields_without_xml() {
  let hash = jenk_hash("CMapData");
  let mut catalog = MetaSchemaCatalog::default();
  catalog.structures.insert(
    hash,
    MetaStructureInfo {
      name_hash: hash,
      structure_key: 1,
      unknown_8: 0,
      unknown_12: 0,
      unknown_28: 0,
      structure_size: 12,
      entries: vec![
        MetaStructureEntry {
          name_hash: jenk_hash("flags"),
          data_offset: 0,
          data_type: 0x15,
          unknown: 0,
          reference_type_index: 0,
          reference_key: 0,
        },
        MetaStructureEntry {
          name_hash: jenk_hash("parentIndex"),
          data_offset: 4,
          data_type: 0x14,
          unknown: 0,
          reference_type_index: 0,
          reference_key: 0,
        },
        MetaStructureEntry {
          name_hash: jenk_hash("scaleXY"),
          data_offset: 8,
          data_type: 0x21,
          unknown: 0,
          reference_type_index: 0,
          reference_key: 0,
        },
      ],
    },
  );
  let mut writer = Writer {
    catalog: &catalog,
    blocks: Vec::new(),
  };
  let bytes = writer
    .structure(hash, &serde_json::json!({"flags": 9, "parent_index": -1, "scale_x_y": 1.25}))
    .unwrap();
  assert_eq!(&bytes[..4], &9u32.to_le_bytes());
  assert_eq!(&bytes[4..8], &(-1i32).to_le_bytes());
  assert_eq!(&bytes[8..], &1.25f32.to_le_bytes());
}

#[test]
fn direct_ymap_binary_preserves_order_and_duplicate_entity_guids() {
  use crate::core::format::gamefile::rsc7::Rsc7Resource;
  let model: Ymap =
    crate::core::format::gamefile::test_support::sample_ymap("parent_refs/vanilla_parent.ymap");
  let mut entities = model.entities.clone();
  entities.reverse();
  entities.push(entities[0].clone());
  let root = jenk_hash("CMapData");
  let entity_type = jenk_hash("CEntityDef");
  let entry = |name_hash, data_offset, data_type, reference_key| MetaStructureEntry {
    name_hash,
    data_offset,
    data_type,
    reference_key,
    unknown: 0,
    reference_type_index: 0,
  };
  let schema = |name_hash, structure_size, entries| MetaStructureInfo {
    name_hash,
    structure_size,
    entries,
    structure_key: 1,
    unknown_8: 0,
    unknown_12: 0,
    unknown_28: 0,
  };
  let mut catalog = MetaSchemaCatalog::default();
  catalog.structures.insert(
    root,
    schema(
      root,
      32,
      vec![
        entry(jenk_hash("flags"), 0, 0x15, 0),
        entry(0x100, 0, 0x07, 0),
        entry(jenk_hash("entities"), 8, 0x52, 0),
      ],
    ),
  );
  catalog
    .structures
    .insert(entity_type, schema(entity_type, 4, vec![entry(jenk_hash("guid"), 0, 0x15, 0)]));
  let bytes = write_ymap(&model, &entities, &catalog).unwrap();
  let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
  let decoded = meta
    .data_blocks
    .iter()
    .filter(|block| block.structure_name_hash == entity_type)
    .map(|block| u32::from_le_bytes(block.data[..4].try_into().unwrap()))
    .collect::<Vec<_>>();
  assert_eq!(decoded, entities.iter().map(|entity| entity.guid).collect::<Vec<_>>());
  assert_eq!(
    u16::from_le_bytes(meta.data_blocks[0].data[16..18].try_into().unwrap()),
    entities.len() as u16
  );
  assert_eq!(write_ymap(&model, &entities, &catalog).unwrap(), bytes);
  assert!(primitive(0x11, &serde_json::json!(256)).is_err());
  assert!(primitive(0x13, &serde_json::json!(65536)).is_err());
  assert!(primitive(0x13, &serde_json::json!(1.5)).is_err());
}

#[test]
#[ignore = "requires local vanilla YMAP schemas"]
fn direct_ymap_writer_roundtrips_local_vanilla() {
  use crate::core::format::gamefile::rsc7::Rsc7Resource;
  use crate::core::format::ymap::binary::ymap_to_model;
  let root =
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap");
  let mut paths =
    std::fs::read_dir(root).unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
  paths.sort();
  let mut catalog = MetaSchemaCatalog::default();
  for path in &paths {
    let bytes = std::fs::read(path).unwrap();
    if let Ok(resource) = Rsc7Resource::decode(&bytes)
      && let Ok(meta) = MetaResource::parse(&resource)
    {
      catalog.add_resource(&meta);
    }
  }
  let mut selected = paths.iter().take(20).collect::<Vec<_>>();
  for pattern in ["occl", "lodlights", "distlodlights", "interior", "grass"] {
    if let Some(path) =
      paths.iter().find(|path| path.file_name().unwrap().to_string_lossy().contains(pattern))
    {
      selected.push(path);
    }
  }
  for path in selected {
    let bytes = std::fs::read(path).unwrap();
    let model = ymap_to_model(&bytes, &catalog.hash_names).unwrap();
    let entities = model.entities.clone();
    let output = write_ymap(&model, &entities, &catalog)
      .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let restored = ymap_to_model(&output, &catalog.hash_names).unwrap();
    let restored_entities = restored.entities.clone();
    assert_eq!(restored, model, "{}", path.display());
    assert_eq!(restored_entities, entities, "{}", path.display());
    assert_eq!(write_ymap(&model, &entities, &catalog).unwrap(), output);
  }
}

#[test]
#[ignore = "requires local vanilla YMAP schemas"]
fn direct_ymap_writer_rejects_container_lods_without_a_schema() {
  use super::super::model::YmapContainerLod;
  use crate::core::format::gamefile::rsc7::Rsc7Resource;
  use crate::core::format::ymap::binary::ymap_to_model;
  // No local YMAP uses containerLods, so none embeds the rage__fwContainerLodDef schema and
  // CodeWalker has no META layout for it; writing them must fail rather than drop them.
  let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("asset/vanilla-cache/latest/ymap/ap1_01_a.ymap");
  let bytes = std::fs::read(path).unwrap();
  let mut catalog = MetaSchemaCatalog::default();
  catalog.add_resource(&MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap());
  let mut model = ymap_to_model(&bytes, &catalog.hash_names).unwrap();
  model.container_lods = vec![YmapContainerLod {
    name: "container".into(),
    parent_index: 0,
  }];
  let entities = model.entities.clone();
  let error = write_ymap(&model, &entities, &catalog).unwrap_err();
  assert!(error.to_string().contains(&format!("{:08X}", jenk_hash("rage__fwContainerLodDef"))));
}

#[test]
fn entity_extents_alone_match_the_decoded_model() {
  let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample");
  for name in [
    "sample.ymap",
    "occlusion/occlusion.ymap",
    "parent_refs/child.ymap",
    "parent_refs/vanilla_parent.ymap",
  ] {
    let bytes = std::fs::read(root.join(name)).unwrap();
    let names = std::collections::HashMap::new();
    let model = super::ymap_to_model(&bytes, &names).unwrap();
    let (minimum, maximum) = super::ymap_entities_extents(&bytes, &names).unwrap();
    assert_eq!(minimum, model.entities_extents_min, "{name}");
    assert_eq!(maximum, model.entities_extents_max, "{name}");
  }
  assert!(super::ymap_entities_extents(b"not a ymap", &std::collections::HashMap::new()).is_err());
}
