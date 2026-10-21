use super::super::model::Ymap;
use super::write_ymap;
use crate::core::format::gamefile::meta::{MetaResource, jenk_hash};
use std::collections::HashMap;

#[test]
fn direct_ymap_binary_preserves_order_and_duplicate_entity_guids() {
  use crate::core::format::gamefile::rsc7::Rsc7Resource;
  let model: Ymap =
    crate::core::format::gamefile::test_support::sample_ymap("parent_refs/vanilla_parent.ymap");
  let mut entities = model.entities.clone();
  entities.reverse();
  entities.push(entities[0].clone());
  let bytes = write_ymap(&model, &entities).unwrap();
  let names = entities
    .iter()
    .map(|entity| (jenk_hash(&entity.archetype_name), entity.archetype_name.clone()));
  let restored = super::ymap_to_model(&bytes, &names.collect()).unwrap();
  assert_eq!(restored.entities, entities);
  // Only the schemas of the written structures are embedded.
  let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
  let embedded = meta.structures.iter().map(|schema| schema.name_hash).collect::<Vec<_>>();
  assert!(embedded.contains(&jenk_hash("CEntityDef")));
  assert!(!embedded.contains(&jenk_hash("CMloInstanceDef")));
  assert!(embedded.is_sorted());
  assert_eq!(write_ymap(&model, &entities).unwrap(), bytes);
}

#[test]
#[ignore = "requires local vanilla YMAP schemas"]
fn direct_ymap_writer_roundtrips_local_vanilla() {
  use crate::core::format::ymap::binary::ymap_to_model;
  let root =
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap");
  let mut paths =
    std::fs::read_dir(root).unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
  paths.sort();
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
    let model = ymap_to_model(&bytes, &HashMap::new()).unwrap();
    let entities = model.entities.clone();
    let output =
      write_ymap(&model, &entities).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let restored = ymap_to_model(&output, &HashMap::new()).unwrap();
    let restored_entities = restored.entities.clone();
    assert_eq!(restored, model, "{}", path.display());
    assert_eq!(restored_entities, entities, "{}", path.display());
    assert_eq!(write_ymap(&model, &entities).unwrap(), output);
  }
}

#[test]
#[ignore = "requires local vanilla YMAP schemas"]
fn direct_ymap_writer_rejects_container_lods_without_a_schema() {
  use super::super::model::YmapContainerLod;
  use crate::core::format::ymap::binary::ymap_to_model;
  // No vanilla YMAP uses containerLods, so the shared schemas lack rage__fwContainerLodDef and
  // CodeWalker has no META layout for it; writing them must fail rather than drop them.
  let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("asset/vanilla-cache/latest/ymap/ap1_01_a.ymap");
  let bytes = std::fs::read(path).unwrap();
  let mut model = ymap_to_model(&bytes, &HashMap::new()).unwrap();
  model.container_lods = vec![YmapContainerLod {
    name: "container".into(),
    parent_index: 0,
  }];
  let entities = model.entities.clone();
  let error = write_ymap(&model, &entities).unwrap_err();
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
