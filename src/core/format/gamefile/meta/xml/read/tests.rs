use super::meta_from_xml;
use crate::core::format::gamefile::{
  meta::{MetaResource, MetaSchemaCatalog},
  rsc7::Rsc7Resource,
  test_support::{sample_ymap_catalog, sample_ymap_xml},
};

#[test]
fn rebuilds_parent_reference_sample_meta_from_xml() {
  let xml = sample_ymap_xml("parent_refs/child.ymap.xml");
  let catalog: &MetaSchemaCatalog = sample_ymap_catalog();
  let rebuilt = meta_from_xml(&xml, catalog).unwrap();
  assert_eq!(rebuilt.root_block_index, 1);
  assert!(!rebuilt.data_blocks.is_empty());
  let resource = rebuilt.to_rsc7(2).unwrap();
  let bytes = resource.encode().unwrap();
  let decoded = Rsc7Resource::decode(&bytes).unwrap();
  let reparsed = MetaResource::parse(&decoded).unwrap();
  assert_eq!(reparsed.root_block_index, rebuilt.root_block_index);
  assert_eq!(reparsed.structures, rebuilt.structures);
  assert_eq!(reparsed.enums, rebuilt.enums);
  assert_eq!(reparsed.data_blocks, rebuilt.data_blocks);
}

#[test]
fn rebuilds_legacy_distant_light_position_elements() {
  let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
  let bytes =
    std::fs::read(base.join("asset/vanilla/ymap/vw_distlodlights_medium010.ymap")).unwrap();
  let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
  let mut catalog = MetaSchemaCatalog::default();
  catalog.add_resource(&meta);
  let xml =
    std::fs::read_to_string(base.join("asset/merged.xml/vw_distlodlights_medium010.ymap.xml"))
      .unwrap();
  let mut root = super::parse_xml(&xml).unwrap();
  let distant = root.children.iter_mut().find(|child| child.name == "DistantLODLightsSOA").unwrap();
  let position = distant.children.iter_mut().find(|child| child.name == "position").unwrap();
  let expected = position.children.len();
  assert!(expected > 0);
  for child in &mut position.children {
    child.name = "XmlPositionChildValueAttr".into();
  }
  let mut blocks = Vec::new();
  super::build_structure(
    distant,
    super::jenk_hash("CDistantLODLight"),
    &catalog.structures,
    &catalog.enums,
    &catalog.hash_names,
    &mut blocks,
  )
  .unwrap();
  let positions = blocks
    .iter()
    .find(|block| block.structure_name_hash == super::jenk_hash("FloatXYZ"))
    .expect("distant light positions were discarded");
  let stride = catalog.structures[&super::jenk_hash("FloatXYZ")].structure_size as usize;
  assert_eq!(positions.data.len(), (expected * stride).next_multiple_of(16));
}

#[test]
fn rejects_incomplete_or_unrecognized_distant_light_positions() {
  let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
  let bytes =
    std::fs::read(base.join("asset/vanilla/ymap/vw_distlodlights_medium010.ymap")).unwrap();
  let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
  let mut catalog = MetaSchemaCatalog::default();
  catalog.add_resource(&meta);
  let xml =
    std::fs::read_to_string(base.join("asset/merged.xml/vw_distlodlights_medium010.ymap.xml"))
      .unwrap();
  for unknown_tag in [false, true] {
    let mut root = super::parse_xml(&xml).unwrap();
    let distant =
      root.children.iter_mut().find(|child| child.name == "DistantLODLightsSOA").unwrap();
    let positions = distant.children.iter_mut().find(|child| child.name == "position").unwrap();
    if unknown_tag {
      positions.children[0].name = "UnknownVector".into();
    } else {
      positions.children.pop();
    }
    let error = super::build_structure(
      distant,
      super::jenk_hash("CDistantLODLight"),
      &catalog.structures,
      &catalog.enums,
      &catalog.hash_names,
      &mut Vec::new(),
    )
    .unwrap_err();
    let expected = if unknown_tag { "unsupported XML item" } else { "counts differ" };
    assert!(error.to_string().contains(expected));
  }
}
