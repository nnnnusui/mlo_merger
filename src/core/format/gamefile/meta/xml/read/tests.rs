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
