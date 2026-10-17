//! Native YMAP → XML → YMAP → XML round trips over the samples under `docs/sample`.

use std::{collections::HashMap, path::Path};

use super::{
  meta::{MetaResource, MetaSchemaCatalog},
  rsc7::Rsc7Resource,
};
use crate::core::format::stream_file::{FileType, StreamFile};

const SAMPLES: [&str; 6] = [
  "sample.ymap",
  "occlusion/occlusion.ymap",
  "parent_refs/vanilla_parent.ymap",
  "parent_refs/resource_a_parent.ymap",
  "parent_refs/resource_b_parent.ymap",
  "parent_refs/child.ymap",
];

#[test]
fn sample_ymaps_survive_binary_xml_binary_xml() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample");
  let names: HashMap<u32, String> =
    crate::core::conversion::load_hash_names_or_empty(&base.join("hash_names.json"));
  for sample in SAMPLES {
    let source = std::fs::read(base.join(sample)).unwrap();
    let meta = MetaResource::parse(&Rsc7Resource::decode(&source).unwrap()).unwrap();
    let mut catalog = MetaSchemaCatalog::default();
    catalog.add_resource(&meta);
    let first = StreamFile::decode_to_xml(FileType::Ymap, &source, &names).unwrap();
    let rebuilt = StreamFile::encode_xml(FileType::Ymap, &first, &catalog).unwrap();
    let second = StreamFile::decode_to_xml(FileType::Ymap, &rebuilt, &names).unwrap();
    assert_eq!(second, first, "{sample}: XML changed after rebuilding the binary");
    assert!(rebuilt == source, "{sample}: rebuilt binary differs from the source");
  }
}
