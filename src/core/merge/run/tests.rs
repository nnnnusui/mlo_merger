use super::*;
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
    output_dir: root.join("output"),
    rebuild_all: true,
    blacklist_config: None,
    overwrites: Default::default(),
    vanilla_overwrites: Default::default(),
  };
  merge
    .run_with_latest_vanilla_files(
      std::slice::from_ref(&vanilla),
      &[("resource".into(), source_path.clone(), None)],
    )
    .unwrap();
  assert!(!root.join("output").join(name).exists());
  let mut edited = model.clone();
  let bit = (!model.content_flags).trailing_zeros();
  assert!(bit < 32);
  edited.content_flags |= 1u32 << bit;
  fs::write(
    &source_path,
    crate::core::format::ymap::binary::write_ymap(&edited, &entities).unwrap(),
  )
  .unwrap();
  merge
    .run_with_latest_vanilla_files(
      std::slice::from_ref(&vanilla),
      &[("resource".into(), source_path.clone(), None)],
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
    .run_with_latest_vanilla_files(&[vanilla], &[("resource".into(), source_path, None)])
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
  fs::write(&deleted, crate::core::format::ymap::binary::write_ymap(&model, &remaining).unwrap())
    .unwrap();
  let merge = MergeYmap {
    vanilla_dir: root.join("vanilla"),
    output_dir: root.join("output"),
    rebuild_all: true,
    blacklist_config: None,
    overwrites: Default::default(),
    vanilla_overwrites: Default::default(),
  };
  for reverse in [false, true] {
    let mut sources =
      vec![("deleted".into(), deleted.clone(), None), ("retained".into(), retained.clone(), None)];
    if reverse {
      sources.reverse();
    }
    merge.run_with_latest_vanilla_files(std::slice::from_ref(&vanilla), &sources).unwrap();
    let bytes = fs::read(root.join("output").join(name)).unwrap();
    let result = ymap_to_model(&bytes, &catalog.hash_names).unwrap().entities;
    assert!(!result.iter().any(|entity| entity.guid == guid));
    assert_eq!(result.len(), remaining.len());
  }
  fs::remove_dir_all(root).unwrap();
}

/// Copies `docs/sample/parent_refs/<sample>` to `path`.
fn copy_sample(
  sample: &str,
  path: &Path,
) {
  let samples = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/parent_refs");
  fs::create_dir_all(path.parent().unwrap()).unwrap();
  fs::copy(samples.join(sample), path).unwrap();
}

/// Writes `model` with `entities` to `path` as an RSC7 binary, using the sample schemas.
fn write_sample(
  path: &Path,
  model: &Ymap,
  entities: &[YmapEntity],
) {
  use crate::core::format::gamefile::{
    meta::{MetaResource, MetaSchemaCatalog},
    rsc7::Rsc7Resource,
  };
  let bytes = fs::read(
    Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/parent_refs/vanilla_parent.ymap"),
  )
  .unwrap();
  let mut catalog = MetaSchemaCatalog::default();
  catalog.add_resource(&MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap());
  fs::create_dir_all(path.parent().unwrap()).unwrap();
  fs::write(path, crate::core::format::ymap::binary::write_ymap(model, entities).unwrap()).unwrap();
}

#[test]
fn merge_emits_only_edited_ymaps_when_rebuilding() {
  use crate::core::format::gamefile::test_support::sample_ymap;
  let root = std::env::temp_dir().join(format!("ymap_edited_only_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let output = root.join("output");
  let vanilla = root.join("vanilla/parent.ymap");
  let (resource_a, resource_b) = (root.join("a/parent.ymap"), root.join("b/parent.ymap"));
  copy_sample("vanilla_parent.ymap", &vanilla);
  copy_sample("vanilla_parent.ymap", &resource_a);
  let merge = MergeYmap {
    vanilla_dir: root.join("vanilla"),
    output_dir: output.clone(),
    rebuild_all: true,
    blacklist_config: None,
    overwrites: Default::default(),
    vanilla_overwrites: Default::default(),
  };
  let run = |sources: &[SourceYmap]| {
    merge.run_with_latest_vanilla_files(std::slice::from_ref(&vanilla), sources).unwrap()
  };
  let a = ("resource_a".to_string(), resource_a.clone(), None);
  let b = ("resource_b".to_string(), resource_b.clone(), None);
  run(std::slice::from_ref(&a));
  assert!(!output.join("parent.ymap").exists());
  copy_sample("vanilla_parent.ymap", &resource_b);
  run(&[a.clone(), b.clone()]);
  assert!(!output.join("parent.ymap").exists());
  let mut edited = sample_ymap("parent_refs/vanilla_parent.ymap");
  edited.entities.first_mut().unwrap().position.x += 1.0;
  write_sample(&resource_a, &edited, &edited.entities);
  run(&[a.clone(), b.clone()]);
  assert!(output.join("parent.ymap").exists());
  fs::copy(&resource_a, &resource_b).unwrap();
  run(&[a.clone(), b.clone()]);
  let duplicates: DuplicateReport = serde_json::from_reader(std::io::BufReader::new(
    fs::File::open(output.join(".duplicates.json")).unwrap(),
  ))
  .unwrap();
  assert_eq!(duplicates.files["parent.ymap"].len(), 1);
  assert_eq!(duplicates.files["parent.ymap"][0].guid, 100);
  assert_eq!(duplicates.files["parent.ymap"][0].ignored[0].reason, "identical_duplicate");
  copy_sample("vanilla_parent.ymap", &resource_a);
  copy_sample("vanilla_parent.ymap", &resource_b);
  run(&[a, b]);
  assert!(!output.join("parent.ymap").exists());
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn merge_deletes_omitted_entities_and_detaches_dependent_children() {
  use crate::core::format::gamefile::test_support::sample_ymap;
  let root = std::env::temp_dir().join(format!("mlo_parent_refs_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let output = root.join("output");
  let vanilla =
    ["parent.ymap", "child.ymap", "dependent.ymap"].map(|name| root.join("vanilla").join(name));
  copy_sample("vanilla_parent.ymap", &vanilla[0]);
  copy_sample("child.ymap", &vanilla[1]);
  copy_sample("child.ymap", &vanilla[2]);
  copy_sample("resource_a_parent.ymap", &root.join("a/parent.ymap"));
  copy_sample("child.ymap", &root.join("a/child.ymap"));
  let deleted = sample_ymap("parent_refs/vanilla_parent.ymap");
  let remaining =
    deleted.entities.iter().filter(|entity| entity.guid != 200).cloned().collect::<Vec<_>>();
  write_sample(&root.join("b/parent.ymap"), &deleted, &remaining);
  MergeYmap {
    vanilla_dir: root.join("vanilla"),
    output_dir: output.clone(),
    rebuild_all: false,
    blacklist_config: None,
    overwrites: Default::default(),
    vanilla_overwrites: Default::default(),
  }
  .run_with_latest_vanilla_files(
    &vanilla,
    &[
      ("resource_a".into(), root.join("a/parent.ymap"), None),
      ("resource_b".into(), root.join("b/parent.ymap"), None),
      ("resource_a".into(), root.join("a/child.ymap"), None),
    ],
  )
  .unwrap();
  let parent = OriginalMap::load(&output.join("parent.ymap")).unwrap();
  assert_eq!(parent.entities.iter().map(|entity| entity.guid).collect::<Vec<_>>(), [100]);
  assert!(!output.join("clone/child.ymap").exists(), "a child of a deleted parent must be rebuilt");
  assert!(!fs::read_to_string(output.join("_copy_targets.txt")).unwrap().contains("child"));
  for name in ["child.ymap", "dependent.ymap"] {
    let child = OriginalMap::load(&output.join(name)).unwrap();
    assert_eq!(child.entities.len(), 1);
    assert_eq!(child.entities[0].parent_index, -1);
    assert_eq!(child.entities[0].flags & 8, 0);
    assert_eq!(child.entities[0].lod_level, "LODTYPES_DEPTH_ORPHANHD");
  }
  fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "requires local vanilla YMAP schemas"]
fn sources_with_a_diff_document_merge_through_it_and_reject_stale_ones() {
  use crate::common::fingerprint::Fingerprint;
  use crate::core::format::gamefile::{
    meta::{MetaResource, MetaSchemaCatalog},
    rsc7::Rsc7Resource,
  };
  use crate::core::format::ymap::{
    binary::ymap_to_model,
    compare::{YmapComparisonDiff, YmapComparisonFile},
  };
  let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap");
  let mut files =
    fs::read_dir(base).unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
  files.sort();
  let file =
    files.iter().find(|file| file.file_stem().unwrap().to_string_lossy().contains("occl")).unwrap();
  let bytes = fs::read(file).unwrap();
  let mut catalog = MetaSchemaCatalog::default();
  catalog.add_resource(&MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap());
  let model = ymap_to_model(&bytes, &catalog.hash_names).unwrap();

  let root = std::env::temp_dir().join(format!("ymap_document_merge_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  fs::create_dir_all(root.join("vanilla")).unwrap();
  fs::create_dir_all(root.join("source")).unwrap();
  let name = file.file_name().unwrap();
  let (vanilla, source) = (root.join("vanilla").join(name), root.join("source").join(name));
  fs::write(&vanilla, &bytes).unwrap();
  fs::write(&source, &bytes).unwrap();
  // The document records a change the source file does not have, so the result shows which
  // was merged.
  let bit = (!model.content_flags).trailing_zeros();
  let flags = model.content_flags | 1u32 << bit;
  let recorded = |path: &Path| {
    let path = path.canonicalize().unwrap();
    let fingerprint = Fingerprint::read(&path, None, false).unwrap();
    YmapComparisonFile {
      path: path.display().to_string(),
      sha256: fingerprint.sha256,
      size: fingerprint.size,
      modified_seconds: fingerprint.modified_seconds,
      modified_nanos: fingerprint.modified_nanos,
    }
  };
  let document = root.join("map.diff.json");
  fs::write(
    &document,
    serde_json::to_vec(&YmapComparisonDiff {
      schema_version: YmapComparisonDiff::SCHEMA_VERSION,
      source: recorded(&source),
      vanilla: recorded(&vanilla),
      tolerances: Default::default(),
      partner: None,
      diffs: crate::core::format::ymap::compare::YmapDiffFields {
        content_flags: Some(flags),
        ..Default::default()
      },
    })
    .unwrap(),
  )
  .unwrap();
  let merge = MergeYmap {
    vanilla_dir: root.join("vanilla"),
    output_dir: root.join("output"),
    rebuild_all: true,
    blacklist_config: None,
    overwrites: Default::default(),
    vanilla_overwrites: Default::default(),
  };
  let sources = [("resource".to_string(), source.clone(), Some(document.clone()))];
  merge.run_with_latest_vanilla_files(std::slice::from_ref(&vanilla), &sources).unwrap();
  let output = fs::read(root.join("output").join(name)).unwrap();
  assert_eq!(ymap_to_model(&output, &catalog.hash_names).unwrap().content_flags, flags);

  fs::write(&source, [bytes.as_slice(), &[0]].concat()).unwrap();
  let error =
    merge.run_with_latest_vanilla_files(std::slice::from_ref(&vanilla), &sources).unwrap_err();
  assert!(error.to_string().contains("rebuild the source cache"), "{error}");
  fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "requires local vanilla YMAP schemas"]
fn lod_light_removals_of_either_ymap_of_a_pair_are_made_in_both() {
  use crate::core::format::gamefile::{
    meta::{MetaResource, MetaSchemaCatalog},
    rsc7::Rsc7Resource,
  };
  use crate::core::format::ymap::binary::{write_ymap, ymap_to_model};
  let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap");
  let names = ["vw_lodlights_large000.ymap", "vw_distlodlights_large000.ymap"];
  let mut catalog = MetaSchemaCatalog::default();
  let bytes = names.map(|name| fs::read(base.join(name)).unwrap());
  for bytes in &bytes {
    catalog.add_resource(&MetaResource::parse(&Rsc7Resource::decode(bytes).unwrap()).unwrap());
  }
  let [lods, distants] =
    bytes.each_ref().map(|bytes| ymap_to_model(bytes, &catalog.hash_names).unwrap());
  assert_eq!(lods.lod_lights.len(), distants.distant_lod_lights.items.len());
  let without = |ymap: &Ymap, removed: &[usize]| {
    let keep = |index: &usize| !removed.contains(index);
    let mut ymap = ymap.clone();
    ymap.lod_lights =
      ymap.lod_lights.into_iter().enumerate().filter(|(i, _)| keep(i)).map(|(_, l)| l).collect();
    ymap.distant_lod_lights.items = ymap
      .distant_lod_lights
      .items
      .into_iter()
      .enumerate()
      .filter(|(i, _)| keep(i))
      .map(|(_, l)| l)
      .collect();
    ymap
  };

  let root = std::env::temp_dir().join(format!("ymap_lod_light_pairs_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let vanilla = names.map(|name| root.join("vanilla").join(name));
  for (path, bytes) in vanilla.iter().zip(&bytes) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
  }
  let write = |path: &Path, ymap: &Ymap| {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, write_ymap(ymap, &ymap.entities).unwrap()).unwrap();
  };
  // `only_distant` ships only the distant lights, leaving the LOD lights vanilla; `both` ships
  // both, removing a different light from each.
  let only_distant = root.join("only_distant").join(names[1]);
  write(&only_distant, &without(&distants, &[1, 3]));
  let both = names.map(|name| root.join("both").join(name));
  write(&both[0], &without(&lods, &[5]));
  write(&both[1], &without(&distants, &[7]));

  let output = root.join("output");
  MergeYmap {
    vanilla_dir: root.join("vanilla"),
    output_dir: output.clone(),
    rebuild_all: true,
    blacklist_config: None,
    overwrites: Default::default(),
    vanilla_overwrites: Default::default(),
  }
  .run_with_latest_vanilla_files(
    &vanilla,
    &[
      ("only_distant".into(), only_distant, None),
      ("both".into(), both[0].clone(), None),
      ("both".into(), both[1].clone(), None),
    ],
  )
  .unwrap();
  let merged = names
    .map(|name| ymap_to_model(&fs::read(output.join(name)).unwrap(), &catalog.hash_names).unwrap());
  let expected = without(&lods, &[1, 3, 5, 7]).lod_lights;
  assert_eq!(merged[0].lod_lights, expected);
  let expected = without(&distants, &[1, 3, 5, 7]).distant_lod_lights.items;
  assert_eq!(merged[1].distant_lod_lights.items, expected);
  fs::remove_dir_all(root).unwrap();
}
