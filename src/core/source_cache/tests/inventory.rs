use super::*;

#[test]
fn builds_source_inventory_and_skips_unchanged_inputs_without_diffing() {
  let root = std::env::temp_dir().join(format!("source_cache_test_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let source = root.join("source");
  let source_resource = source.join("resource_a");
  let second_resource = source.join("resource_b");
  let vanilla = root.join("vanilla");
  let derived = root.join("vanilla-cache");
  let output = root.join("source-cache");
  fs::create_dir_all(source_resource.join("stream")).unwrap();
  fs::create_dir_all(second_resource.join("stream")).unwrap();
  fs::create_dir_all(vanilla.join("0000-base/ymap")).unwrap();
  fs::write(source_resource.join("fxmanifest.lua"), b"fx_version 'cerulean'").unwrap();
  fs::write(second_resource.join("fxmanifest.lua"), b"fx_version 'cerulean'").unwrap();
  fs::write(source_resource.join("stream/unmatched.ymap"), b"source bytes").unwrap();
  fs::write(second_resource.join("stream/unmatched.ymap"), b"other source bytes").unwrap();
  fs::write(source_resource.join("stream/unmatched.ybn"), b"source ybn").unwrap();
  fs::write(second_resource.join("stream/unmatched.ybn"), b"other source ybn").unwrap();
  let raw_manifest = VanillaCacheManifest {
    format_version: 1,
    game_dir: "game".into(),
    versions: vec![CacheVersion {
      id: "0000-base".into(),
      parent: None,
      archives: vec![],
      changes: BTreeMap::new(),
      unchanged: 0,
    }],
  };
  write_json(&vanilla.join("cache_info.json"), &raw_manifest).unwrap();

  let build = || BuildSourceCache {
    source_dir: source.clone(),
    output_dir: output.clone(),
    vanilla_dir: vanilla.clone(),
    vanilla_cache_dir: derived.clone(),
    force: false,
  };
  assert!(build().run().unwrap());
  assert!(output.join("source_cache_info.json").is_file());
  assert!(output.join("stream_conflicts.json").is_file());
  assert!(output.join("vanilla_ymaps_to_read.json").is_file());
  assert!(!output.join("diff_cache_info.json").exists());
  let info: SourceCacheMetadata =
    serde_json::from_reader(fs::File::open(output.join("source_cache_info.json")).unwrap())
      .unwrap();
  let resource_info = &info.resources["resource_a"];
  assert_eq!(resource_info.files_by_format[".ymap"].len(), 1);
  assert!(resource_info.files_by_format[".ymap"][0].vanilla.is_none());
  assert_eq!(info.resources.len(), 2);
  assert_eq!(info.conflict_count, 0);
  let report: SourceCacheConflictReport =
    serde_json::from_reader(fs::File::open(output.join("stream_conflicts.json")).unwrap()).unwrap();
  assert!(report.report.conflicts.is_empty());
  assert_eq!(report.source_conflicts.as_ref().unwrap().values().map(Vec::len).sum::<usize>(), 2);
  assert_eq!(info.format_version, 3);
  let merge_inputs = crate::core::source_cache::load_merge_inputs(&output).unwrap();
  assert!(merge_inputs.ymap.is_empty());
  assert!(merge_inputs.ybn.is_empty());
  let vanilla_ymaps_to_read: BTreeSet<String> =
    serde_json::from_reader(fs::File::open(output.join("vanilla_ymaps_to_read.json")).unwrap())
      .unwrap();
  assert!(vanilla_ymaps_to_read.is_empty());
  assert!(!build().run().unwrap());

  write_json(&output.join("stream_conflicts.json"), &report.report).unwrap();
  assert!(build().run().unwrap());
  assert!(!build().run().unwrap());
  for (name, file_name) in [("resource_a", "cached.ybn"), ("resource_b", "CACHED.YBN")] {
    fs::create_dir_all(output.join("resources").join(name).join("nested")).unwrap();
    fs::write(
      output.join("resources").join(name).join("nested").join(file_name),
      b"cache-only file",
    )
    .unwrap();
  }
  assert!(build().run().unwrap());
  let cache_report: SourceCacheConflictReport =
    serde_json::from_reader(fs::File::open(output.join("stream_conflicts.json")).unwrap()).unwrap();
  assert_eq!(cache_report.report.input_dir, output.to_string_lossy());
  assert_eq!(cache_report.report.scanned_file_count, 2);
  assert_eq!(cache_report.report.conflict_count, 1);
  assert_eq!(cache_report.report.conflicts[".ybn"][0].file_name, "cached.ybn");
  assert_eq!(
    cache_report.report.conflicts[".ybn"][0].paths,
    ["resources/resource_a/nested/cached.ybn", "resources/resource_b/nested/CACHED.YBN"]
  );
  assert_eq!(cache_report.source_conflicts, report.source_conflicts);
  fs::create_dir_all(output.join("_old/timestamp/resource_c")).unwrap();
  fs::write(output.join("_old/timestamp/resource_c/cached.ybn"), b"old bytes").unwrap();
  assert!(!build().run().unwrap());
  fs::remove_file(output.join("resources/resource_b/nested/CACHED.YBN")).unwrap();
  assert!(build().run().unwrap());
  let cache_report: SourceCacheConflictReport =
    serde_json::from_reader(fs::File::open(output.join("stream_conflicts.json")).unwrap()).unwrap();
  assert!(cache_report.report.conflicts.is_empty());
  assert_eq!(cache_report.report.scanned_file_count, 1);
  assert_eq!(cache_report.source_conflicts, report.source_conflicts);
  assert!(!build().run().unwrap());

  fs::remove_file(output.join("vanilla_ymaps_to_read.json")).unwrap();
  assert!(build().run().unwrap());

  let mut relationships: serde_json::Value =
    serde_json::from_reader(fs::File::open(derived.join("ymap_relationships.json")).unwrap())
      .unwrap();
  relationships["children_by_parent_hash"] = serde_json::json!({"00000001": ["child.ymap"]});
  write_json(&derived.join("ymap_relationships.json"), &relationships).unwrap();
  assert!(build().run().unwrap());
  fs::remove_dir_all(root).unwrap();
}
