use super::*;
use crate::core::format::ymap::diff::reference_hash;
use std::collections::HashMap;

#[test]
#[ignore = "requires local vanilla YMAP schemas"]
fn latest_ymap_merge_writes_binary_without_xml() {
  use crate::core::format::gamefile::{
    meta::{MetaResource, MetaSchemaCatalog},
    rsc7::Rsc7Resource,
  };
  use crate::core::format::ymap::binary::ymap_to_model;
  let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap");
  let mut files =
    fs::read_dir(base).unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
  files.sort();
  let root = std::env::temp_dir().join(format!("ymap_direct_merge_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  fs::create_dir_all(root.join("vanilla")).unwrap();
  fs::create_dir_all(root.join("source")).unwrap();
  let source =
    files.iter().find(|file| file.file_stem().unwrap().to_string_lossy().contains("occl")).unwrap();
  let bytes = fs::read(source).unwrap();
  let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
  let mut compatible = MetaSchemaCatalog::default();
  compatible.add_resource(&meta);
  let model = ymap_to_model(&bytes, &compatible.hash_names).unwrap();
  let entities = model.entities.clone();
  let name = source.file_name().unwrap();
  let vanilla = root.join("vanilla").join(name);
  let source_path = root.join("source").join(name);
  fs::write(&vanilla, &bytes).unwrap();
  fs::write(&source_path, &bytes).unwrap();
  let merge = MergeYmap {
    vanilla_dir: root.join("vanilla"),
    mod_dir: root.join("source"),
    mod_ymap_dir: root.join("source"),
    output_dir: root.join("output"),
    rebuild_all: true,
    blacklist_config: None,
  };
  merge
    .run_with_latest_vanilla_files(
      std::slice::from_ref(&vanilla),
      Some(&[("resource".into(), source_path.clone())]),
    )
    .unwrap();
  assert!(!root.join("output").join(name).exists());
  let mut edited = model.clone();
  let bit = (!model.content_flags).trailing_zeros();
  assert!(bit < 32);
  edited.content_flags |= 1u32 << bit;
  fs::write(
    &source_path,
    crate::core::format::ymap::binary::write_ymap(&edited, &entities, &compatible).unwrap(),
  )
  .unwrap();
  merge
    .run_with_latest_vanilla_files(
      std::slice::from_ref(&vanilla),
      Some(&[("resource".into(), source_path.clone())]),
    )
    .unwrap();
  let output = fs::read(root.join("output").join(name)).unwrap();
  assert!(output.starts_with(b"RSC7"));
  assert!(!root.join("output").join(format!("{}.xml", name.to_string_lossy())).exists());
  let restored = ymap_to_model(&output, &compatible.hash_names).unwrap();
  let restored_entities = restored.entities.clone();
  assert_eq!(restored.content_flags, edited.content_flags);
  assert_eq!(restored_entities, entities);
  fs::write(&source_path, bytes).unwrap();
  merge
    .run_with_latest_vanilla_files(&[vanilla], Some(&[("resource".into(), source_path)]))
    .unwrap();
  assert!(!root.join("output").join(name).exists());
  fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "requires local vanilla YMAP schemas"]
fn native_ymap_merge_deletion_wins_over_retained_entities_in_both_orders() {
  use crate::core::format::gamefile::{
    meta::{MetaResource, MetaSchemaCatalog},
    rsc7::Rsc7Resource,
  };
  use crate::core::format::ymap::binary::ymap_to_model;
  let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap");
  let mut files =
    fs::read_dir(base).unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
  files.sort();
  let sample = files
    .iter()
    .find(|path| {
      ymap_to_model(&fs::read(path).unwrap(), &HashMap::new())
        .is_ok_and(|model| !model.entities.is_empty())
    })
    .unwrap();
  let bytes = fs::read(sample).unwrap();
  let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
  let mut catalog = MetaSchemaCatalog::default();
  catalog.add_resource(&meta);
  let model = ymap_to_model(&bytes, &catalog.hash_names).unwrap();
  let entities = model.entities.clone();
  let guid = entities[0].guid;
  let remaining = entities.iter().filter(|entity| entity.guid != guid).cloned().collect::<Vec<_>>();
  let root = std::env::temp_dir().join(format!("ymap_delete_native_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  for directory in ["vanilla", "deleted", "retained"] {
    fs::create_dir_all(root.join(directory)).unwrap();
  }
  let name = sample.file_name().unwrap();
  let vanilla = root.join("vanilla").join(name);
  let deleted = root.join("deleted").join(name);
  let retained = root.join("retained").join(name);
  fs::write(&vanilla, &bytes).unwrap();
  fs::write(&retained, &bytes).unwrap();
  fs::write(
    &deleted,
    crate::core::format::ymap::binary::write_ymap(&model, &remaining, &catalog).unwrap(),
  )
  .unwrap();
  let merge = MergeYmap {
    vanilla_dir: root.join("vanilla"),
    mod_dir: root.clone(),
    mod_ymap_dir: root.clone(),
    output_dir: root.join("output"),
    rebuild_all: true,
    blacklist_config: None,
  };
  for reverse in [false, true] {
    let mut sources =
      vec![("deleted".into(), deleted.clone()), ("retained".into(), retained.clone())];
    if reverse {
      sources.reverse();
    }
    merge.run_with_latest_vanilla_files(std::slice::from_ref(&vanilla), Some(&sources)).unwrap();
    let bytes = fs::read(root.join("output").join(name)).unwrap();
    let result = ymap_to_model(&bytes, &catalog.hash_names).unwrap().entities;
    assert!(!result.iter().any(|entity| entity.guid == guid));
    assert_eq!(result.len(), remaining.len());
  }
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn merge_emits_only_edited_ymaps_when_rebuilding() {
  let root = std::env::temp_dir().join(format!("ymap_edited_only_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let vanilla = root.join("vanilla");
  let mods = root.join("mods");
  let output = root.join("output");
  fs::create_dir_all(&vanilla).unwrap();
  fs::create_dir_all(&mods).unwrap();
  let sample =
    Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/parent_refs/vanilla_parent.ymap.xml");
  fs::copy(&sample, vanilla.join("parent.ymap.xml")).unwrap();
  fs::copy(&sample, mods.join("resource_a___parent.ymap.xml")).unwrap();
  let merge = MergeYmap {
    vanilla_dir: vanilla,
    mod_dir: mods.clone(),
    mod_ymap_dir: mods.clone(),
    output_dir: output.clone(),
    rebuild_all: true,
    blacklist_config: None,
  };
  merge.run().unwrap();
  assert!(!output.join("parent.ymap.xml").exists());
  fs::copy(&sample, mods.join("resource_b___parent.ymap.xml")).unwrap();
  merge.run().unwrap();
  assert!(!output.join("parent.ymap.xml").exists());
  let mut edited = parse_ymap_xml(&sample).unwrap();
  edited.entities.first_mut().unwrap().position.x += 1.0;
  let xml: XmlYmap = edited.into();
  fs::write(mods.join("resource_a___parent.ymap.xml"), quick_xml::se::to_string(&xml).unwrap())
    .unwrap();
  merge.run().unwrap();
  assert!(output.join("parent.ymap.xml").exists());
  fs::write(
    mods.join("resource_b___parent.ymap.xml"),
    fs::read(mods.join("resource_a___parent.ymap.xml")).unwrap(),
  )
  .unwrap();
  merge.run().unwrap();
  let duplicates: DuplicateReport = serde_json::from_reader(std::io::BufReader::new(
    fs::File::open(output.join(".duplicates.json")).unwrap(),
  ))
  .unwrap();
  assert_eq!(duplicates.files["parent.ymap"].len(), 1);
  assert_eq!(duplicates.files["parent.ymap"][0].guid, 100);
  assert_eq!(duplicates.files["parent.ymap"][0].ignored[0].reason, "identical_duplicate");
  fs::copy(&sample, mods.join("resource_b___parent.ymap.xml")).unwrap();
  fs::copy(&sample, mods.join("resource_a___parent.ymap.xml")).unwrap();
  merge.run().unwrap();
  assert!(!output.join("parent.ymap.xml").exists());
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn merge_deletes_omitted_entities_and_detaches_dependent_children() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let staging = std::env::temp_dir().join(format!("mlo_parent_refs_{}", std::process::id()));
  let _ = fs::remove_dir_all(&staging);
  let samples = base.join("docs/sample/parent_refs");
  let vanilla_dir = staging.join("vanilla");
  let mod_dir = staging.join("mods");
  let mod_ymap_dir = staging.join("mod-binaries");
  let output = staging.join("output/merged.xml");
  fs::create_dir_all(&vanilla_dir).unwrap();
  fs::create_dir_all(&mod_dir).unwrap();
  fs::create_dir_all(&mod_ymap_dir).unwrap();
  fs::copy(samples.join("vanilla_parent.ymap.xml"), vanilla_dir.join("parent.ymap.xml")).unwrap();
  fs::copy(samples.join("child.ymap.xml"), vanilla_dir.join("dependent.ymap.xml")).unwrap();
  fs::copy(
    samples.join("resource_a_parent.ymap.xml"),
    mod_dir.join("resource_a___parent.ymap.xml"),
  )
  .unwrap();
  let mut deleted_parent = parse_ymap_xml(&samples.join("vanilla_parent.ymap.xml")).unwrap();
  if let Some(index) = deleted_parent.entities.iter().position(|entity| entity.guid == 200) {
    deleted_parent.entities.remove(index);
  }
  let deleted_parent: XmlYmap = deleted_parent.into();
  fs::write(
    mod_dir.join("resource_b___parent.ymap.xml"),
    quick_xml::se::to_string(&deleted_parent).unwrap(),
  )
  .unwrap();
  fs::copy(samples.join("child.ymap.xml"), mod_dir.join("resource_a___child.ymap.xml")).unwrap();
  fs::write(mod_ymap_dir.join("resource_a___child.ymap"), b"placeholder clone binary").unwrap();
  let (cache, _) = VanillaParentCache::update(&vanilla_dir).unwrap();
  assert_eq!(
    cache
      .children(&vanilla_dir, &[reference_hash("parent")].into_iter().collect())
      .iter()
      .map(|(_, path)| path.file_name().unwrap().to_str().unwrap())
      .collect::<Vec<_>>(),
    ["dependent.ymap.xml"]
  );
  MergeYmap {
    vanilla_dir,
    mod_dir,
    mod_ymap_dir,
    output_dir: output.clone(),
    rebuild_all: false,
    blacklist_config: None,
  }
  .run()
  .unwrap();
  let parent = OriginalMap::load(&output.join("parent.ymap.xml")).unwrap();
  assert_eq!(parent.entities.iter().map(|entity| entity.guid).collect::<Vec<_>>(), [100]);
  let clone = output.join("clone/child.ymap");
  assert!(!clone.exists(), "a child of a deleted parent must be rebuilt");
  assert!(!fs::read_to_string(output.join("_copy_targets.txt")).unwrap().contains("child"));
  for name in ["child.ymap.xml", "dependent.ymap.xml"] {
    let child = OriginalMap::load(&output.join(name)).unwrap();
    assert_eq!(child.entities.len(), 1);
    assert_eq!(child.entities[0].parent_index, -1);
    assert_eq!(child.entities[0].flags & 8, 0);
    assert_eq!(child.entities[0].lod_level, "LODTYPES_DEPTH_ORPHANHD");
  }
  fs::remove_dir_all(staging).unwrap();
}

#[cfg(test)]
fn parse_ymap_xml(file_path: &Path) -> Result<Ymap, Box<dyn std::error::Error>> {
  let xml_content = fs::read_to_string(file_path)?;
  let xml_ymap: XmlYmap = quick_xml::de::from_str(&xml_content)?;
  if let Some(error) = &xml_ymap.instanced_data.error {
    return Err(
      std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("{} contains an instancedData error: {error}", file_path.display()),
      )
      .into(),
    );
  }
  Ok(xml_ymap.into())
}
