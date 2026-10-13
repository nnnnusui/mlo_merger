use super::*;
use crate::core::vanilla::{CacheVersion, FileChange, read_ymap, write_json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[test]
fn builds_latest_files_through_selected_stage_and_reuses_valid_output() {
  let root = std::env::temp_dir().join(format!("derived_vanilla_cache_{}", std::process::id()));
  let vanilla = root.join("vanilla");
  let output = root.join("cache");
  fs::create_dir_all(vanilla.join("0000-base/ybn")).unwrap();
  fs::create_dir_all(vanilla.join("0001-patch/ybn")).unwrap();
  fs::write(vanilla.join("0000-base/ybn/map.ybn"), b"base").unwrap();
  fs::write(vanilla.join("0001-patch/ybn/map.ybn"), b"patch").unwrap();
  let artifact = |stage: &str, content: &[u8]| CachedFile {
    sha256: hex::encode(Sha256::digest(content)),
    object: format!("{stage}/ybn/map.ybn"),
    source: format!("{stage}.rpf/map.ybn"),
  };
  let manifest = VanillaCacheManifest {
    format_version: 1,
    game_dir: "game".into(),
    versions: vec![
      CacheVersion {
        id: "0000-base".into(),
        parent: None,
        archives: vec![],
        changes: BTreeMap::from([(
          "map.ybn".into(),
          FileChange {
            previous_sha256: None,
            file: artifact("0000-base", b"base"),
          },
        )]),
        unchanged: 0,
      },
      CacheVersion {
        id: "0001-patch".into(),
        parent: Some("0000-base".into()),
        archives: vec![],
        changes: BTreeMap::from([(
          "map.ybn".into(),
          FileChange {
            previous_sha256: Some(hex::encode(Sha256::digest(b"base"))),
            file: artifact("0001-patch", b"patch"),
          },
        )]),
        unchanged: 0,
      },
    ],
  };
  write_json(&vanilla.join("cache_info.json"), &manifest).unwrap();

  let built = BuildVanillaCache {
    vanilla_dir: vanilla.clone(),
    output_dir: output.clone(),
    through_version: Some("base".into()),
    force: false,
  }
  .run()
  .unwrap();
  assert!(built.rebuilt);
  assert_eq!(built.latest_version, "0000-base");
  assert_eq!(built.summary(), "Built vanilla derived cache through 0000-base");
  assert_eq!(fs::read(output.join("latest/ybn/map.ybn")).unwrap(), b"base");
  let relationships: YmapRelationshipIndex =
    serde_json::from_reader(fs::File::open(output.join("ymap_relationships.json")).unwrap())
      .unwrap();
  assert_eq!(relationships.version, "0000-base");
  assert!(relationships.children_by_parent_hash.is_empty());
  let latest_link = output.join("latest/ybn/map.ybn");
  assert!(fs::symlink_metadata(&latest_link).unwrap().file_type().is_symlink());
  let target = fs::read_link(&latest_link).unwrap();
  assert!(!target.is_absolute());
  assert_eq!(
    latest_link.parent().unwrap().join(target).canonicalize().unwrap(),
    vanilla.join("0000-base/ybn/map.ybn").canonicalize().unwrap()
  );
  let derived: DerivedManifest =
    serde_json::from_reader(fs::File::open(output.join("cache_info.json")).unwrap()).unwrap();
  assert_eq!(derived.latest_version, "0000-base");
  let timestamp_key = "0000-base/ybn/map.ybn";
  let source_file = vanilla.join(timestamp_key);
  let changed_time =
    fs::metadata(&source_file).unwrap().modified().unwrap() + std::time::Duration::from_secs(2);
  fs::File::options()
    .write(true)
    .open(&source_file)
    .unwrap()
    .set_times(fs::FileTimes::new().set_modified(changed_time))
    .unwrap();

  BuildVanillaCache {
    vanilla_dir: root.join("vanilla"),
    output_dir: output.clone(),
    through_version: Some("base".into()),
    force: false,
  }
  .run()
  .unwrap();
  let refreshed: DerivedManifest =
    serde_json::from_reader(fs::File::open(output.join("cache_info.json")).unwrap()).unwrap();
  assert_ne!(
    derived.vanilla_input_timestamps[timestamp_key],
    refreshed.vanilla_input_timestamps[timestamp_key]
  );
  assert_eq!(
    refreshed.vanilla_input_timestamps[timestamp_key],
    source_timestamps(&vanilla, &resolve_files(&manifest, 0).unwrap()).unwrap()[timestamp_key]
  );
  assert!(fs::symlink_metadata(&latest_link).unwrap().file_type().is_symlink());

  let reused = BuildVanillaCache {
    vanilla_dir: root.join("vanilla"),
    output_dir: output.clone(),
    through_version: Some("0000".into()),
    force: false,
  }
  .run()
  .unwrap();
  assert!(!reused.rebuilt);
  assert_eq!(reused.latest_version, "0000-base");
  assert_eq!(reused.summary(), "Vanilla derived cache is current through 0000-base");
  assert_eq!(fs::read(output.join("latest/ybn/map.ybn")).unwrap(), b"base");

  fs::remove_file(output.join("latest/ybn/map.ybn")).unwrap();
  BuildVanillaCache {
    vanilla_dir: root.join("vanilla"),
    output_dir: output.clone(),
    through_version: Some("base".into()),
    force: false,
  }
  .run()
  .unwrap();
  assert_eq!(fs::read(output.join("latest/ybn/map.ybn")).unwrap(), b"base");

  let latest = BuildVanillaCache {
    vanilla_dir: root.join("vanilla"),
    output_dir: output.clone(),
    through_version: None,
    force: false,
  }
  .run()
  .unwrap();
  assert!(latest.rebuilt);
  assert_eq!(latest.latest_version, "0001-patch");
  assert_eq!(fs::read(output.join("latest/ybn/map.ybn")).unwrap(), b"patch");
  let relationships: YmapRelationshipIndex =
    serde_json::from_reader(fs::File::open(output.join("ymap_relationships.json")).unwrap())
      .unwrap();
  assert_eq!(relationships.version, "0001-patch");
  let (_, raw_revision) = crate::core::vanilla::load_manifest_with_revision(&vanilla).unwrap();
  let loaded = load_cache(&output, &raw_revision).unwrap();
  assert_eq!(loaded.latest_version, "0001-patch");
  assert_eq!(loaded.files["map.ybn"].version, "0001-patch");
  assert_eq!(loaded.relationships.version, "0001-patch");
  let mut digest = Sha256::new();
  for name in ["cache_info.json", "ymap_relationships.json"] {
    digest.update(name.as_bytes());
    digest.update(fs::read(output.join(name)).unwrap());
  }
  assert_eq!(loaded.revision, hex::encode(digest.finalize()));
  assert!(load_cache(&output, "stale").err().unwrap().to_string().contains("stale"));
  let relationship_path = output.join("ymap_relationships.json");
  let relationship_bytes = fs::read(&relationship_path).unwrap();
  let mut stale_relationships: YmapRelationshipIndex =
    serde_json::from_slice(&relationship_bytes).unwrap();
  stale_relationships.version = "0000-base".into();
  write_json(&relationship_path, &stale_relationships).unwrap();
  assert!(
    load_cache(&output, &raw_revision)
      .err()
      .unwrap()
      .to_string()
      .contains("relationship index is stale")
  );
  fs::write(&relationship_path, relationship_bytes).unwrap();
  fs::remove_file(output.join("latest/ybn/map.ybn")).unwrap();
  assert!(
    load_cache(&output, &raw_revision)
      .err()
      .unwrap()
      .to_string()
      .contains("latest file is missing")
  );
  fs::remove_dir(output.join("latest/ybn")).unwrap();
  assert!(
    load_cache(&output, &raw_revision).err().unwrap().to_string().contains("missing latest/ybn")
  );
  fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "requires generated asset/vanilla-cache latest YMAP links"]
fn direct_parent_reader_matches_latest_ymap_model() {
  let path =
    Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap/airfield.ymap");
  let ymap = read_ymap(&path).unwrap();
  assert_eq!(ymap_parent_hash(&path).unwrap(), reference_hash(&ymap.parent));
}
