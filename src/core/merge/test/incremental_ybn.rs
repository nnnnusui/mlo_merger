use super::*;

#[test]
fn merge_dispatches_latest_vanilla_ybn_and_writes_omit_list() {
  let root = std::env::temp_dir().join(format!("merge_latest_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let source_dir = root.join("source");
  let resource_dir = source_dir.join("resource_a");
  let second_resource_dir = source_dir.join("resource_b");
  let source_stream = resource_dir.join("stream");
  let second_source_stream = second_resource_dir.join("stream");
  let vanilla_dir = root.join("vanilla");
  let vanilla_cache_dir = root.join("vanilla-cache");
  let source_cache_dir = root.join("source-cache");
  let output_dir = root.join("merged");
  fs::create_dir_all(&source_stream).unwrap();
  fs::create_dir_all(&second_source_stream).unwrap();
  fs::write(resource_dir.join("fxmanifest.lua"), []).unwrap();
  fs::write(second_resource_dir.join("fxmanifest.lua"), []).unwrap();
  let xml = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/sample/ybn_conflicts/geometry_bvh.ybn.xml"
  ));
  let bytes = crate::core::format::ybn::xml::xml_to_ybn(xml).unwrap();
  fs::write(source_stream.join("collision.ybn"), &bytes).unwrap();
  fs::write(second_source_stream.join("collision.ybn"), &bytes).unwrap();

  let object = "0000-base/ybn/collision.ybn";
  let vanilla_file = vanilla_dir.join(object);
  fs::create_dir_all(vanilla_file.parent().unwrap()).unwrap();
  fs::write(&vanilla_file, &bytes).unwrap();
  let sha256 = format!("{:x}", Sha256::digest(&bytes));
  write_json(
    &vanilla_dir.join("cache_info.json"),
    &VanillaCacheManifest {
      format_version: 1,
      game_dir: root.join("game"),
      versions: vec![CacheVersion {
        id: "0000-base".into(),
        parent: None,
        archives: vec![],
        changes: BTreeMap::from([(
          "collision.ybn".into(),
          FileChange {
            previous_sha256: None,
            file: CachedFile {
              sha256,
              object: object.into(),
              source: "base.rpf/collision.ybn".into(),
            },
          },
        )]),
        unchanged: 0,
      }],
    },
  )
  .unwrap();

  let independent_object = "0000-base/ybn/independent.ybn";
  fs::write(vanilla_dir.join(independent_object), &bytes).unwrap();
  for stream in [&source_stream, &second_source_stream] {
    fs::write(stream.join("independent.ybn"), &bytes).unwrap();
  }
  let mut manifest: VanillaCacheManifest =
    serde_json::from_reader(fs::File::open(vanilla_dir.join("cache_info.json")).unwrap()).unwrap();
  manifest.versions[0].changes.insert(
    "independent.ybn".into(),
    FileChange {
      previous_sha256: None,
      file: CachedFile {
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        object: independent_object.into(),
        source: "base.rpf/independent.ybn".into(),
      },
    },
  );
  write_json(&vanilla_dir.join("cache_info.json"), &manifest).unwrap();

  Merge {
    source_dir: source_dir.clone(),
    vanilla_dir: vanilla_dir.clone(),
    vanilla_cache_dir: vanilla_cache_dir.clone(),
    source_cache_dir: source_cache_dir.clone(),
    output_dir: output_dir.clone(),
    overwrite_dir: root.join("overwrite"),
    exclusions: None,
    force: false,
  }
  .run()
  .unwrap();

  assert!(vanilla_cache_dir.join("latest/ybn/collision.ybn").is_file());
  assert!(source_cache_dir.join("source_cache_info.json").is_file());
  let merged =
    crate::core::format::ybn::read_ybn(&fs::read(output_dir.join("ybn/collision.ybn")).unwrap())
      .unwrap();
  let source = crate::core::format::ybn::read_ybn(&bytes).unwrap();
  let diff = crate::core::format::ybn::diff::YbnDiff::extract_from(&source, &merged).unwrap();
  assert!(diff.bound_diffs.is_empty());
  assert!(diff.polygon_diffs.is_empty());
  assert_eq!(
    fs::read_to_string(output_dir.join("_omit.txt")).unwrap().trim(),
    "resource_a/stream/collision.ybn\nresource_a/stream/independent.ybn\nresource_b/stream/collision.ybn\nresource_b/stream/independent.ybn"
  );
  let deployment = crate::core::deploy::Deploy {
    merged_dir: output_dir.clone(),
    source_cache_dir: source_cache_dir.clone(),
    output_dir: root.join("deployed"),
    force: false,
  };
  assert_eq!(deployment.run().unwrap().copied, 2);
  let moved_files = "resource_a/stream/collision.ybn\nresource_a/stream/independent.ybn\nresource_b/stream/collision.ybn\nresource_b/stream/independent.ybn";
  assert_eq!(fs::read_to_string(root.join("deployed/files.txt")).unwrap(), moved_files);
  let list_modified = fs::metadata(root.join("deployed/files.txt")).unwrap().modified().unwrap();
  assert_eq!(
    fs::read(root.join("deployed/stream/ybn/merged/collision.ybn")).unwrap(),
    fs::read(output_dir.join("ybn/collision.ybn")).unwrap(),
  );
  assert!(!root.join("deployed/stream/ybn/clone").exists());
  assert_eq!(deployment.run().unwrap().copied, 0);
  assert_eq!(
    fs::metadata(root.join("deployed/files.txt")).unwrap().modified().unwrap(),
    list_modified
  );
  let read_cache = || {
    serde_json::from_reader::<_, super::super::super::incremental::MergeMetadata>(
      fs::File::open(output_dir.join("merge_cache_info.json")).unwrap(),
    )
    .unwrap()
  };
  let initial = read_cache();
  assert_eq!(initial.files.len(), 2);
  let independent = output_dir.join("ybn/independent.ybn");
  let modified = fs::metadata(&independent).unwrap().modified().unwrap();
  let cache_modified =
    fs::metadata(output_dir.join("merge_cache_info.json")).unwrap().modified().unwrap();
  Merge {
    source_dir: source_dir.clone(),
    vanilla_dir: vanilla_dir.clone(),
    vanilla_cache_dir: vanilla_cache_dir.clone(),
    source_cache_dir: source_cache_dir.clone(),
    output_dir: output_dir.clone(),
    overwrite_dir: root.join("overwrite"),
    exclusions: None,
    force: false,
  }
  .run()
  .unwrap();
  assert_eq!(
    fs::metadata(output_dir.join("merge_cache_info.json")).unwrap().modified().unwrap(),
    cache_modified
  );
  fs::File::options()
    .write(true)
    .open(source_cache_dir.join("resources/resource_a/independent.ybn"))
    .unwrap()
    .set_times(
      fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(10)),
    )
    .unwrap();
  Merge {
    source_dir: source_dir.clone(),
    vanilla_dir: vanilla_dir.clone(),
    vanilla_cache_dir: vanilla_cache_dir.clone(),
    source_cache_dir: source_cache_dir.clone(),
    output_dir: output_dir.clone(),
    overwrite_dir: root.join("overwrite"),
    exclusions: None,
    force: false,
  }
  .run()
  .unwrap();
  assert_eq!(
    read_cache().files["independent.ybn"].merged_at,
    initial.files["independent.ybn"].merged_at
  );
  assert_eq!(fs::metadata(&independent).unwrap().modified().unwrap(), modified);
  let mut changed_source = bytes.clone();
  changed_source.extend_from_slice(b"changed source fingerprint");
  crate::core::format::ybn::read_ybn(&changed_source).unwrap();
  let cached_source = source_cache_dir.join("resources/resource_a/collision.ybn");
  fs::write(&cached_source, &changed_source).unwrap();
  Merge {
    source_dir: source_dir.clone(),
    vanilla_dir: vanilla_dir.clone(),
    vanilla_cache_dir: vanilla_cache_dir.clone(),
    source_cache_dir: source_cache_dir.clone(),
    output_dir: output_dir.clone(),
    overwrite_dir: root.join("overwrite"),
    exclusions: None,
    force: false,
  }
  .run()
  .unwrap();
  let changed = read_cache();
  assert_ne!(changed.files["collision.ybn"].merged_at, initial.files["collision.ybn"].merged_at);
  assert_eq!(
    changed.files["independent.ybn"].merged_at,
    initial.files["independent.ybn"].merged_at
  );
  assert_eq!(fs::metadata(&independent).unwrap().modified().unwrap(), modified);
  fs::write(output_dir.join("ybn/collision.ybn"), b"broken output").unwrap();
  Merge {
    source_dir: source_dir.clone(),
    vanilla_dir: vanilla_dir.clone(),
    vanilla_cache_dir: vanilla_cache_dir.clone(),
    source_cache_dir: source_cache_dir.clone(),
    output_dir: output_dir.clone(),
    overwrite_dir: root.join("overwrite"),
    exclusions: None,
    force: false,
  }
  .run()
  .unwrap();
  crate::core::format::ybn::read_ybn(&fs::read(output_dir.join("ybn/collision.ybn")).unwrap())
    .unwrap();
  assert_eq!(
    read_cache().files["independent.ybn"].merged_at,
    initial.files["independent.ybn"].merged_at
  );
  let before_vanilla = read_cache();
  let mut updated_vanilla = bytes.clone();
  updated_vanilla.extend_from_slice(b"changed vanilla fingerprint");
  fs::write(&vanilla_file, &updated_vanilla).unwrap();
  manifest.versions[0].changes.get_mut("collision.ybn").unwrap().file.sha256 =
    format!("{:x}", Sha256::digest(&updated_vanilla));
  write_json(&vanilla_dir.join("cache_info.json"), &manifest).unwrap();
  Merge {
    source_dir: source_dir.clone(),
    vanilla_dir: vanilla_dir.clone(),
    vanilla_cache_dir: vanilla_cache_dir.clone(),
    source_cache_dir: source_cache_dir.clone(),
    output_dir: output_dir.clone(),
    overwrite_dir: root.join("overwrite"),
    exclusions: None,
    force: false,
  }
  .run()
  .unwrap();
  let after_vanilla = read_cache();
  assert_ne!(
    after_vanilla.files["collision.ybn"].merged_at,
    before_vanilla.files["collision.ybn"].merged_at
  );
  assert_eq!(
    after_vanilla.files["independent.ybn"].merged_at,
    before_vanilla.files["independent.ybn"].merged_at
  );
  fs::remove_file(&independent).unwrap();
  Merge {
    source_dir: source_dir.clone(),
    vanilla_dir: vanilla_dir.clone(),
    vanilla_cache_dir: vanilla_cache_dir.clone(),
    source_cache_dir: source_cache_dir.clone(),
    output_dir: output_dir.clone(),
    overwrite_dir: root.join("overwrite"),
    exclusions: None,
    force: false,
  }
  .run()
  .unwrap();
  assert!(independent.is_file());
  let before_force = read_cache();
  Merge {
    source_dir: source_dir.clone(),
    vanilla_dir: vanilla_dir.clone(),
    vanilla_cache_dir: vanilla_cache_dir.clone(),
    source_cache_dir: source_cache_dir.clone(),
    output_dir: output_dir.clone(),
    overwrite_dir: root.join("overwrite"),
    exclusions: None,
    force: true,
  }
  .run()
  .unwrap();
  let forced = read_cache();
  assert!(
    forced
      .files
      .iter()
      .all(|(name, record)| record.merged_at != before_force.files[name].merged_at)
  );
  let preserved = fs::read(output_dir.join("merge_cache_info.json")).unwrap();
  let preserved_output = fs::read(output_dir.join("ybn/collision.ybn")).unwrap();
  fs::write(&cached_source, b"invalid source file").unwrap();
  assert!(
    Merge {
      source_dir: source_dir.clone(),
      vanilla_dir: vanilla_dir.clone(),
      vanilla_cache_dir: vanilla_cache_dir.clone(),
      source_cache_dir: source_cache_dir.clone(),
      output_dir: output_dir.clone(),
      overwrite_dir: root.join("overwrite"),
      exclusions: None,
      force: false
    }
    .run()
    .is_err()
  );
  assert_eq!(fs::read(output_dir.join("merge_cache_info.json")).unwrap(), preserved);
  assert_eq!(fs::read(output_dir.join("ybn/collision.ybn")).unwrap(), preserved_output);
  fs::write(cached_source, changed_source).unwrap();
  fs::remove_dir_all(&second_resource_dir).unwrap();
  Merge {
    source_dir: source_dir.clone(),
    vanilla_dir: vanilla_dir.clone(),
    vanilla_cache_dir: vanilla_cache_dir.clone(),
    source_cache_dir: source_cache_dir.clone(),
    output_dir: output_dir.clone(),
    overwrite_dir: root.join("overwrite"),
    exclusions: None,
    force: false,
  }
  .run()
  .unwrap();
  assert!(read_cache().files.is_empty());
  assert!(!output_dir.join("ybn/collision.ybn").exists());
  assert!(!independent.exists());
  assert!(fs::read_to_string(output_dir.join("_omit.txt")).unwrap().is_empty());
  assert_eq!(deployment.run().unwrap().copied, 2);
  assert_eq!(
    fs::read_to_string(root.join("deployed/files.txt")).unwrap(),
    "resource_a/stream/collision.ybn\nresource_a/stream/independent.ybn"
  );
  assert!(root.join("deployed/stream/ybn/clone/resource_a/collision.ybn").is_file());
  fs::remove_dir_all(root).unwrap();
}
