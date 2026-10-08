use super::*;
use crate::core::vanilla::{CacheVersion, FileChange, read_ymap, write_json};
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
    sha256: format!("{:x}", Sha256::digest(content)),
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
            previous_sha256: Some(format!("{:x}", Sha256::digest(b"base"))),
            file: artifact("0001-patch", b"patch"),
          },
        )]),
        unchanged: 0,
      },
    ],
  };
  write_json(&vanilla.join("cache_info.json"), &manifest).unwrap();

  BuildVanillaCache {
    vanilla_dir: vanilla.clone(),
    output_dir: output.clone(),
    through_version: Some("base".into()),
    force: false,
  }
  .run()
  .unwrap();
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

  BuildVanillaCache {
    vanilla_dir: root.join("vanilla"),
    output_dir: output.clone(),
    through_version: Some("0000".into()),
    force: false,
  }
  .run()
  .unwrap();
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

  BuildVanillaCache {
    vanilla_dir: root.join("vanilla"),
    output_dir: output.clone(),
    through_version: None,
    force: false,
  }
  .run()
  .unwrap();
  assert_eq!(fs::read(output.join("latest/ybn/map.ybn")).unwrap(), b"patch");
  let relationships: YmapRelationshipIndex =
    serde_json::from_reader(fs::File::open(output.join("ymap_relationships.json")).unwrap())
      .unwrap();
  assert_eq!(relationships.version, "0001-patch");
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
