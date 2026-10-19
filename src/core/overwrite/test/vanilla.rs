use super::*;

#[test]
fn vanilla_overwrites_are_read_from_latest_ymaps_only() {
  let root = std::env::temp_dir().join(format!("vanilla_overwrite_load_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let file = serde_json::json!({
    "schema_version": VanillaOverwrite::SCHEMA_VERSION,
    "vanilla": {"sha256": "a", "size": 1},
    "diffs": {"flags": 1},
  });
  let write = |path: &str| {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec(&file).unwrap()).unwrap();
  };
  write("vanilla/latest/ymap/Map.ymap.diff.json");
  write("vanilla/latest/ymap/notes.json");
  write("vanilla/latest/ybn/map.ybn.diff.json");
  write("vanilla/0000-base/ymap/other.ymap.diff.json");

  let overwrites = Overwrites::load(&root).unwrap();
  assert_eq!(overwrites.vanilla_files().keys().collect::<Vec<_>>(), ["map.ymap"]);
  let overwrite = overwrites.vanilla_file("MAP.ymap").unwrap();
  let fingerprint = |sha256: &str| OverwriteFingerprint {
    sha256: sha256.into(),
    size: 1,
  };
  assert!(overwrite.diffs_for(&fingerprint("a"), "map").is_some());
  assert!(overwrite.diffs_for(&fingerprint("b"), "map").is_none());

  write("vanilla/latest/ymap/map.ymap.diff.json");
  let mut stale = file.clone();
  stale["schema_version"] = 0.into();
  fs::write(
    root.join("vanilla/latest/ymap/map.ymap.diff.json"),
    serde_json::to_vec(&stale).unwrap(),
  )
  .unwrap();
  assert!(Overwrites::load(&root).is_err());
  fs::remove_dir_all(root).unwrap();
}
