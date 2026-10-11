use super::*;
use std::fs;

#[test]
fn reuses_stats_and_invalidates_edits_inputs_and_topology() {
  let root = std::env::temp_dir().join(format!("overwrite_reuse_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let source = native_resources(&root, &["resource"]);
  let cache = root.join("cache");
  let overwrite = root.join("overwrite");
  fs::create_dir_all(&cache).unwrap();
  fs::create_dir_all(overwrite.join("resource/nested")).unwrap();
  let inventory = inventory(&source, &cache).unwrap();
  let (export, _) =
    export::uncached_snapshot("resource", "nested/child.ymap", &inventory["resource"]).unwrap();
  let mut edit = YmapLightEditDocument {
    format: "mlo_lod_light_edits".into(),
    schema_version: 1,
    resource: export.resource.clone(),
    path: export.path.clone(),
    files: export.files.clone(),
    operations: vec![LightOperation::Remove {
      light_id: export.lights[0].id.clone(),
    }],
  };
  let diff = overwrite.join("resource/nested/child.ymap.diff.json");
  crate::core::vanilla::write_json(&diff, &edit).unwrap();
  let load = |force| PreparedEdits::load_cached(&source, &cache, &overwrite, force).unwrap();
  let (_, first) = load(false);
  assert!(!first.reused());
  first.save(&cache).unwrap();
  let cache_path = cache.join("overwrite_cache_info.json");
  let timestamp = fs::metadata(&cache_path).unwrap().modified().unwrap();
  let (_, second) = load(false);
  assert!(second.reused());
  second.save(&cache).unwrap();
  assert_eq!(fs::metadata(&cache_path).unwrap().modified().unwrap(), timestamp);
  fs::OpenOptions::new()
    .write(true)
    .open(&diff)
    .unwrap()
    .set_times(
      fs::FileTimes::new()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(10)),
    )
    .unwrap();
  let (_, touched) = load(false);
  assert!(touched.reused());
  touched.save(&cache).unwrap();
  assert!(!load(true).1.reused());
  edit.operations.push(LightOperation::Remove {
    light_id: export.lights[1].id.clone(),
  });
  crate::core::vanilla::write_json(&diff, &edit).unwrap();
  let (changed, changed_cache) = load(false);
  assert!(!changed_cache.reused());
  changed_cache.save(&cache).unwrap();
  let original_cache = fs::read(&cache_path).unwrap();
  fs::write(&diff, b"invalid").unwrap();
  assert!(PreparedEdits::load_cached(&source, &cache, &overwrite, false).is_err());
  assert_eq!(fs::read(&cache_path).unwrap(), original_cache);
  crate::core::vanilla::write_json(&diff, &edit).unwrap();
  let parent = source.join("resource/stream/nested/parent.ymap");
  let original_parent = fs::read(&parent).unwrap();
  fs::write(&parent, b"changed native input").unwrap();
  assert!(changed.verify().is_err());
  assert!(PreparedEdits::load_cached(&source, &cache, &overwrite, false).is_err());
  assert_eq!(fs::read(&cache_path).unwrap(), original_cache);
  fs::write(&parent, original_parent).unwrap();
  native_resources(&root, &["second_resource"]);
  assert!(!load(false).1.reused());
  fs::remove_file(&diff).unwrap();
  let (empty, removed) = load(false);
  assert!(!removed.reused());
  assert!(empty.names.is_empty());
  removed.save(&cache).unwrap();
  assert!(load(false).1.reused());
  fs::write(&cache_path, b"broken cache").unwrap();
  assert!(!load(false).1.reused());
  fs::remove_dir_all(root).unwrap();
}
