use super::*;
use crate::core::{
  format::ymap::diff::{BatchKey, reference_hash},
  format::ymap::model::{GrassInstance, GrassInstanceBatch},
};
use std::collections::{BTreeSet, HashMap, HashSet};

#[test]
#[ignore = "requires local vanilla YMAP schemas"]
fn latest_ymap_merge_writes_binary_without_xml() {
  use crate::core::format::gamefile::{
    meta_resource::{MetaResource, MetaSchemaCatalog},
    meta_xml::ymap_to_model_with_entities,
    resource_file::Rsc7Resource,
  };
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
  let (model, entities) = ymap_to_model_with_entities(&bytes, &compatible.hash_names).unwrap();
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
  let (restored, restored_entities) =
    ymap_to_model_with_entities(&output, &compatible.hash_names).unwrap();
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
    meta_resource::{MetaResource, MetaSchemaCatalog},
    meta_xml::ymap_to_model_with_entities,
    resource_file::Rsc7Resource,
  };
  let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap");
  let mut files =
    fs::read_dir(base).unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
  files.sort();
  let sample = files
    .iter()
    .find(|path| {
      ymap_to_model_with_entities(&fs::read(path).unwrap(), &HashMap::new()).is_ok_and(
        |(model, entities)| !entities.is_empty() && model.entity_map.len() == entities.len(),
      )
    })
    .unwrap();
  let bytes = fs::read(sample).unwrap();
  let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
  let mut catalog = MetaSchemaCatalog::default();
  catalog.add_resource(&meta);
  let (model, entities) = ymap_to_model_with_entities(&bytes, &catalog.hash_names).unwrap();
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
    let (_, result) = ymap_to_model_with_entities(&bytes, &catalog.hash_names).unwrap();
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
  edited.entity_map.values_mut().next().unwrap().position.x += 1.0;
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
  deleted_parent.entity_map.shift_remove(&200);
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

#[test]
#[ignore = "requires local vanilla XML, extracted mods, and the original merge log"]
fn merge_logged_unsupported_ymaps() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let old_log = fs::read_to_string(base.join("asset/log/mlo_merger_20261002_154241.log")).unwrap();
  let mut current = None;
  let mut targets = BTreeSet::new();
  for line in old_log.lines() {
    if let Some((_, path)) = line.split_once("Processing YMAP: ") {
      current = Some(Path::new(path).file_name().unwrap().to_str().unwrap().to_string());
    }
    if ["parent(", "physics_dictionaries(", "instanced_data("]
      .iter()
      .any(|field| line.contains(&format!("skip unsupported changes: {field}")))
    {
      targets.insert(current.clone().expect("warning without YMAP context"));
    }
  }
  assert!(!targets.is_empty(), "no unsupported-field fixtures found");
  let staging = std::env::temp_dir().join(format!("mlo_merge_metadata_{}", std::process::id()));
  let vanilla_dir = staging.join("vanilla.xml");
  let mod_dir = staging.join("mod.xml");
  fs::create_dir_all(&vanilla_dir).unwrap();
  fs::create_dir_all(&mod_dir).unwrap();
  let output_dir = base.join("asset/merge_validation/unsupported_fields/merged.xml");
  let report_dir = output_dir.parent().unwrap();
  fs::create_dir_all(report_dir).unwrap();
  simplelog::WriteLogger::init(
    log::LevelFilter::Info,
    simplelog::Config::default(),
    fs::File::create(report_dir.join("merge.log")).unwrap(),
  )
  .unwrap();
  for name in &targets {
    fs::copy(base.join("asset/vanilla/ymap.xml").join(name), vanilla_dir.join(name))
      .unwrap_or_else(|error| panic!("Native vanilla XML missing for {name}: {error}"));
  }
  let sources = collect_modded_ymaps_map(&base.join("asset/extracted.xml")).unwrap();
  let mut mod_count = 0;
  for name in &targets {
    let references = sources.get(name).unwrap_or_else(|| panic!("no mod references for {name}"));
    for source in references {
      fs::copy(&source.mod_ymap_path, mod_dir.join(source.mod_ymap_path.file_name().unwrap()))
        .unwrap();
      mod_count += 1;
    }
  }
  let runner = MergeYmap {
    vanilla_dir,
    mod_dir,
    mod_ymap_dir: base.join("asset/extracted"),
    output_dir: output_dir.clone(),
    rebuild_all: true,
    blacklist_config: Some(base.join("asset/blacklist.toml")),
  };
  runner.run().unwrap();
  let mut records = Vec::new();
  for name in &targets {
    let original = parse_ymap_xml(&runner.vanilla_dir.join(name)).unwrap();
    let mods = sources[name]
      .iter()
      .map(|source| parse_ymap_xml(&source.mod_ymap_path).unwrap())
      .collect::<Vec<_>>();
    let merged = parse_ymap_xml(&output_dir.join(name)).unwrap();
    let expected_parent = mods
      .iter()
      .find(|modified| reference_hash(&modified.parent) != reference_hash(&original.parent))
      .map(|modified| &modified.parent)
      .unwrap_or(&original.parent);
    assert_eq!(reference_hash(&merged.parent), reference_hash(expected_parent), "parent: {name}");
    let original_refs =
      original.physics_dictionaries.iter().map(|name| reference_hash(name)).collect::<HashSet<_>>();
    let modified_refs = mods
      .iter()
      .map(|modified| {
        modified
          .physics_dictionaries
          .iter()
          .map(|name| reference_hash(name))
          .collect::<HashSet<_>>()
      })
      .collect::<Vec<_>>();
    let mut expected_refs = original_refs
      .iter()
      .filter(|hash| modified_refs.iter().all(|refs| refs.contains(hash)))
      .copied()
      .collect::<HashSet<_>>();
    for refs in &modified_refs {
      expected_refs.extend(refs.difference(&original_refs).copied());
    }
    let actual_refs =
      merged.physics_dictionaries.iter().map(|name| reference_hash(name)).collect::<HashSet<_>>();
    assert_eq!(actual_refs, expected_refs, "physics dictionaries: {name}");
    assert_eq!(
      actual_refs.len(),
      merged.physics_dictionaries.len(),
      "duplicate dictionaries: {name}"
    );
    verify_grass(&original, &mods, &merged, name);
    let instances = merged
      .instanced_data
      .grass_instance_list
      .iter()
      .map(|batch| batch.instances.len())
      .sum::<usize>();
    records.push(serde_json::json!({
      "ymap": name, "mods": sources[name].iter().map(|source| &source.mod_name).collect::<Vec<_>>(),
      "parent": merged.parent, "physics_dictionaries": merged.physics_dictionaries.len(),
      "grass_batches": merged.instanced_data.grass_instance_list.len(), "grass_instances": instances,
    }));
  }
  fs::write(report_dir.join("results.json"), serde_json::to_vec_pretty(&records).unwrap()).unwrap();
  let merge_log = fs::read_to_string(report_dir.join("merge.log")).unwrap();
  assert!(
    !merge_log.contains("skip unsupported changes"),
    "unsupported changes remain in targeted merge log"
  );
  fs::remove_dir_all(staging).unwrap();
  eprintln!(
    "Verified {} targeted YMAPs across {mod_count} mod references; output: {}",
    targets.len(),
    output_dir.display()
  );
}

fn positions(batch: &GrassInstanceBatch) -> BTreeMap<Vec<u32>, GrassInstance> {
  batch
    .instances
    .iter()
    .map(|instance| {
      (
        instance
          .position
          .iter()
          .map(|value| if *value == 0.0 { 0 } else { value.to_bits() })
          .collect(),
        instance.clone(),
      )
    })
    .collect()
}

fn verify_grass(
  original: &Ymap,
  mods: &[Ymap],
  merged: &Ymap,
  name: &str,
) {
  let original_batches = original
    .instanced_data
    .grass_instance_list
    .iter()
    .map(|batch| (BatchKey::from_batch(batch), positions(batch)))
    .collect::<HashMap<_, _>>();
  let mod_batches = mods
    .iter()
    .map(|modified| {
      modified
        .instanced_data
        .grass_instance_list
        .iter()
        .map(|batch| (BatchKey::from_batch(batch), positions(batch)))
        .collect::<HashMap<_, _>>()
    })
    .collect::<Vec<_>>();
  let keys = original_batches
    .keys()
    .chain(mod_batches.iter().flat_map(|batches| batches.keys()))
    .cloned()
    .collect::<HashSet<_>>();
  let mut expected = HashMap::new();
  for key in keys {
    let before = original_batches.get(&key);
    if before.is_some() && mod_batches.iter().any(|batches| !batches.contains_key(&key)) {
      continue;
    }
    let empty = BTreeMap::new();
    let before = before.unwrap_or(&empty);
    let mut instances = before.clone();
    instances.retain(|position, _| {
      mod_batches
        .iter()
        .all(|batches| batches.get(&key).is_some_and(|instances| instances.contains_key(position)))
    });
    let removed = before
      .keys()
      .filter(|position| !instances.contains_key(*position))
      .cloned()
      .collect::<HashSet<_>>();
    let mut selected = HashSet::new();
    for batches in &mod_batches {
      if let Some(modified) = batches.get(&key) {
        for (position, instance) in modified {
          if !removed.contains(position)
            && before.get(position) != Some(instance)
            && selected.insert(position.clone())
          {
            instances.insert(position.clone(), instance.clone());
          }
        }
      }
    }
    expected.insert(key, instances);
  }
  let actual = merged
    .instanced_data
    .grass_instance_list
    .iter()
    .map(|batch| (BatchKey::from_batch(batch), positions(batch)))
    .collect::<HashMap<_, _>>();
  assert!(
    actual == expected,
    "grass deltas differ for {name}: actual {} batches, expected {}",
    actual.len(),
    expected.len()
  );
  assert_eq!(
    actual.len(),
    merged.instanced_data.grass_instance_list.len(),
    "duplicate grass batches: {name}"
  );
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
