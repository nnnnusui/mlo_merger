use super::*;
use crate::core::vanilla::{CacheVersion, FileChange, read_ymap, write_json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Build 1, which stops at the `update` stage.
fn no_dlc() -> Option<crate::core::config::gamebuild::GameBuild> {
  let builds = crate::core::config::gamebuild::GameBuilds::parse("[gamebuild]\n1 = []").unwrap();
  Some(builds.resolve("1").unwrap())
}

#[test]
fn builds_latest_files_through_selected_stage_and_reuses_valid_output() {
  let root = std::env::temp_dir().join(format!("derived_vanilla_cache_{}", std::process::id()));
  let vanilla = root.join("vanilla");
  let output = root.join("cache");
  fs::create_dir_all(vanilla.join("0000-update/ybn")).unwrap();
  fs::create_dir_all(vanilla.join("0001-patch/ybn")).unwrap();
  fs::write(vanilla.join("0000-update/ybn/map.ybn"), b"base").unwrap();
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
        id: "0000-update".into(),
        parent: None,
        archives: vec![],
        changes: BTreeMap::from([(
          "map.ybn".into(),
          FileChange {
            previous_sha256: None,
            file: artifact("0000-update", b"base"),
          },
        )]),
        unchanged: 0,
      },
      CacheVersion {
        id: "0001-patch".into(),
        parent: Some("0000-update".into()),
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
    gamebuild: no_dlc(),
    force: false,
  }
  .run()
  .unwrap();
  assert!(built.rebuilt);
  assert_eq!(built.latest_version, "0000-update");
  assert_eq!(built.summary(), "Built vanilla derived cache through 0000-update");
  assert_eq!(built.changes.added, ["0000-update/ybn/map.ybn".into()].into());
  let latest = |output: &Path| {
    let files = latest_files(output).unwrap();
    assert_eq!(files.len(), 1);
    fs::read(&files[0].path).unwrap()
  };
  assert_eq!(latest(&output), b"base");
  assert!(!output.join("latest").exists());
  let relationships: YmapRelationshipIndex =
    serde_json::from_reader(fs::File::open(output.join("ymap_relationships.json")).unwrap())
      .unwrap();
  assert_eq!(relationships.version, "0000-update");
  assert!(relationships.children_by_parent.is_empty());
  let derived: DerivedManifest =
    serde_json::from_reader(fs::File::open(output.join("cache_info.json")).unwrap()).unwrap();
  assert_eq!(derived.latest_version, "0000-update");
  // The archive is named relative to the cache, so the two can move together.
  assert_eq!(derived.vanilla_dir, "../vanilla");
  assert_eq!(derived.files["map.ybn"].object, "0000-update/ybn/map.ybn");
  assert_eq!(
    latest_files(&output).unwrap()[0].path,
    fs::canonicalize(&root).unwrap().join("vanilla/0000-update/ybn/map.ybn")
  );
  let timestamp_key = "0000-update/ybn/map.ybn";
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
    gamebuild: no_dlc(),
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

  let reused = BuildVanillaCache {
    vanilla_dir: root.join("vanilla"),
    output_dir: output.clone(),
    gamebuild: no_dlc(),
    force: false,
  }
  .run()
  .unwrap();
  assert!(!reused.rebuilt);
  assert_eq!(reused.latest_version, "0000-update");
  assert_eq!(reused.summary(), "Vanilla derived cache is current through 0000-update");
  assert_eq!(latest(&output), b"base");

  // A cache moved away from its archive is built again, naming the archive where it is now.
  let moved = root.join("moved/cache");
  fs::create_dir_all(moved.parent().unwrap()).unwrap();
  fs::rename(&output, &moved).unwrap();
  let rebuilt = BuildVanillaCache {
    vanilla_dir: root.join("vanilla"),
    output_dir: moved.clone(),
    gamebuild: no_dlc(),
    force: false,
  }
  .run()
  .unwrap();
  assert!(rebuilt.rebuilt);
  assert_eq!(latest(&moved), b"base");
  fs::rename(&moved, &output).unwrap();

  let latest = BuildVanillaCache {
    vanilla_dir: root.join("vanilla"),
    output_dir: output.clone(),
    gamebuild: None,
    force: false,
  }
  .run()
  .unwrap();
  assert!(latest.rebuilt);
  assert_eq!(latest.latest_version, "0001-patch");
  // The replaced stage's file leaves the latest files and the newer stage's takes its place.
  assert_eq!(latest.changes.removed, ["0000-update/ybn/map.ybn".into()].into());
  assert_eq!(latest.changes.added, ["0001-patch/ybn/map.ybn".into()].into());
  assert_eq!(latest_files(&output).unwrap()[0].version, "0001-patch");
  assert_eq!(fs::read(vanilla.join(&latest_files(&output).unwrap()[0].object)).unwrap(), b"patch");
  let relationships: YmapRelationshipIndex =
    serde_json::from_reader(fs::File::open(output.join("ymap_relationships.json")).unwrap())
      .unwrap();
  assert_eq!(relationships.version, "0001-patch");
  let (_, raw_revision) = crate::core::vanilla::load_manifest_with_revision(&vanilla).unwrap();
  let loaded = load_cache(&output, &raw_revision).unwrap();
  assert_eq!(loaded.latest_version, "0001-patch");
  assert_eq!(loaded.files["map.ybn"].version, "0001-patch");
  assert_eq!(loaded.relationships.version, "0001-patch");
  // The revision covers both files through the SHA-256 of each.
  let mut digest = Sha256::new();
  digest.update(b"cache_info.json");
  digest.update(hex::encode(Sha256::digest(fs::read(output.join("cache_info.json")).unwrap())));
  digest.update(b"ymap_relationships.json");
  digest.update(Sha256::digest(fs::read(output.join("ymap_relationships.json")).unwrap()));
  assert_eq!(loaded.revision, hex::encode(digest.finalize()));
  assert!(load_cache(&output, "stale").err().unwrap().to_string().contains("stale"));
  let relationship_path = output.join("ymap_relationships.json");
  let relationship_bytes = fs::read(&relationship_path).unwrap();
  let mut stale_relationships: YmapRelationshipIndex =
    serde_json::from_slice(&relationship_bytes).unwrap();
  stale_relationships.version = "0000-update".into();
  write_json(&relationship_path, &stale_relationships).unwrap();
  assert!(
    load_cache(&output, &raw_revision)
      .err()
      .unwrap()
      .to_string()
      .contains("relationship index is stale")
  );
  fs::write(&relationship_path, relationship_bytes).unwrap();
  fs::remove_file(vanilla.join("0001-patch/ybn/map.ybn")).unwrap();
  assert!(
    load_cache(&output, &raw_revision)
      .err()
      .unwrap()
      .to_string()
      .contains("Latest vanilla file is missing")
  );
  fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "requires generated asset/vanilla-cache"]
fn direct_parent_reader_matches_latest_ymap_model() {
  let path = super::test_support::asset_latest_file("airfield.ymap");
  let ymap = read_ymap(&path).unwrap();
  assert_eq!(ymap_parent_hash(&path).unwrap(), reference_hash(&ymap.parent));
}

#[test]
#[ignore = "YMAP version diff generation is disabled"]
fn writes_ymap_version_diffs_that_rebuild_each_version() {
  use crate::core::format::ymap::binary::write_ymap;
  use std::collections::HashMap;

  let root = std::env::temp_dir().join(format!("vanilla_ymap_diffs_{}", std::process::id()));
  let vanilla = root.join("vanilla");
  let output = root.join("cache");
  let base =
    crate::core::format::gamefile::test_support::sample_ymap("parent_refs/vanilla_parent.ymap");
  let mut update = base.clone();
  update.entities.remove(0);
  update.entities[0].lod_dist += 0.0001;
  update.streaming_extents_max.x += 1.0;
  update.physics_dictionaries = vec!["added".into()];
  let mut patch = update.clone();
  patch.flags += 1;
  let stages = [("0000-base", &base), ("0001-update", &update), ("0002-patch", &patch)];
  let mut versions = Vec::new();
  let mut previous_sha256 = None;
  for (index, (id, ymap)) in stages.iter().enumerate() {
    let bytes = write_ymap(ymap, &ymap.entities).unwrap();
    let object = format!("{id}/ymap/map.ymap");
    fs::create_dir_all(vanilla.join(id).join("ymap")).unwrap();
    fs::write(vanilla.join(&object), &bytes).unwrap();
    let sha256 = hex::encode(Sha256::digest(&bytes));
    versions.push(CacheVersion {
      id: id.to_string(),
      parent: index.checked_sub(1).map(|parent| stages[parent].0.to_string()),
      archives: vec![],
      changes: BTreeMap::from([(
        "Map.ymap".into(),
        FileChange {
          previous_sha256: previous_sha256.replace(sha256.clone()),
          file: CachedFile {
            sha256,
            object,
            source: format!("{id}.rpf/map.ymap"),
          },
        },
      )]),
      unchanged: 0,
    });
  }
  write_json(
    &vanilla.join("cache_info.json"),
    &VanillaCacheManifest {
      format_version: 1,
      game_dir: "game".into(),
      versions,
    },
  )
  .unwrap();
  let build = |gamebuild: Option<crate::core::config::gamebuild::GameBuild>| {
    BuildVanillaCache {
      vanilla_dir: vanilla.clone(),
      output_dir: output.clone(),
      gamebuild,
      force: false,
    }
    .run()
    .unwrap()
  };

  build(no_dlc());
  let history = ymap_history(&output, "MAP.ymap").unwrap().unwrap();
  assert_eq!(history.versions.len(), 2);
  assert!(history.diff_path(1).ends_with("diff/0001-update/ymap/map.ymap.diff.json"));
  // Marked so a copy can be told from a document taken again.
  let marked = format!("{}\n", fs::read_to_string(history.diff_path(1)).unwrap());
  fs::write(history.diff_path(1), &marked).unwrap();

  assert!(build(None).rebuilt);
  let history = ymap_history(&output, "map.ymap").unwrap().unwrap();
  let ids = history.versions.iter().map(|version| version.version.as_str()).collect::<Vec<_>>();
  assert_eq!(ids, ["0000-base", "0001-update", "0002-patch"]);
  // The diff to the unchanged update is kept, not taken again.
  assert_eq!(fs::read_to_string(history.diff_path(1)).unwrap(), marked);
  assert_eq!(history.read_diff(2).unwrap().diffs.flags, Some(patch.flags));
  let names = HashMap::new();
  for (index, (_, ymap)) in stages.iter().enumerate() {
    let expected = crate::core::format::ymap::binary::ymap_to_model(
      &fs::read(vanilla.join(&history.versions[index].object)).unwrap(),
      &names,
    )
    .unwrap();
    assert_eq!(history.rebuild(&vanilla, index, &names).unwrap(), expected);
    assert_eq!(expected.entities.len(), ymap.entities.len());
  }
  assert!(!build(None).rebuilt);

  fs::remove_file(history.diff_path(2)).unwrap();
  assert!(build(None).rebuilt);
  assert!(history.diff_path(2).is_file());
  fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "YMAP version diff generation is disabled; also requires the raw vanilla archive"]
fn ymap_version_diffs_rebuild_every_vanilla_version() {
  use crate::core::format::ymap::binary::ymap_to_model;

  let vanilla = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla");
  let output = std::env::temp_dir().join(format!("vanilla_ymap_history_{}", std::process::id()));
  BuildVanillaCache {
    vanilla_dir: vanilla.clone(),
    output_dir: output.clone(),
    gamebuild: None,
    force: true,
  }
  .run()
  .unwrap();
  let names = crate::core::conversion::load_hash_names_or_empty(&vanilla.join("hash_names.json"));
  let index: serde_json::Value =
    serde_json::from_slice(&fs::read(output.join("ymap_versions.json")).unwrap()).unwrap();
  let (mut versions, mut failures) = (0, Vec::new());
  for name in index["ymaps"].as_object().unwrap().keys() {
    let history = ymap_history(&output, name).unwrap().unwrap();
    for index in 1..history.versions.len() {
      versions += 1;
      let rebuilt = history.rebuild(&vanilla, index, &names).unwrap();
      let path = vanilla.join(&history.versions[index].object);
      let expected = ymap_to_model(&fs::read(path).unwrap(), &names).unwrap();
      if rebuilt != expected {
        failures.push(format!("{name} {}", history.versions[index].version));
      }
    }
  }
  println!("{} of {versions} versions rebuilt exactly", versions - failures.len());
  fs::remove_dir_all(output).unwrap();
  assert!(failures.is_empty(), "{failures:#?}");
}
