use super::*;
use crate::common::changes::StoreChanges;
use crate::core::overwrite::IgnoredOverwrite;
use crate::core::vanilla::{
  CacheVersion, CachedFile, FileChange, VanillaCacheManifest, write_json,
};
use crate::platform::fs;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[test]
fn report_adds_overwrite_warnings_to_the_changes() {
  use crate::core::overwrite::IgnoredOverwriteReason;
  let report = PipelineReport {
    changes: PipelineChanges::default(),
    warnings: crate::core::overwrite_cache::OverwriteWarnings {
      ignored_overwrites: vec![IgnoredOverwrite {
        resource: Some("resource".into()),
        file: "maps/map.ymap".into(),
        reason: IgnoredOverwriteReason::SourceChanged,
      }],
    },
  };
  let empty = r#"{"added":[],"modified":[],"removed":[]}"#;
  assert_eq!(
    serde_json::to_string(&report).unwrap(),
    format!(
      r#"{{"vanilla_cache":{empty},"source_cache":{empty},"merged":{empty},"ignored_overwrites":[{{"resource":"resource","file":"maps/map.ymap","reason":"source_changed"}}]}}"#
    )
  );
}

#[test]
fn pipeline_routes_resources_through_merge_and_deploy_and_reuses_vanilla() {
  let root = std::env::temp_dir().join(format!("default_pipeline_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let pipeline = Pipeline {
    source_dir: root.join("input"),
    game_dir: root.join("missing-game"),
    magic: "magic.dat".into(),
    vanilla_dir: root.join("vanilla"),
    vanilla_cache_dir: root.join("vanilla-cache"),
    source_cache_dir: root.join("source-cache"),
    exclusions: None,
    merged_dir: root.join("merged"),
    output_dir: root.join("merged_mlo"),
    gamebuild: None,
    force: false,
    overwrite_dir: None,
    overwrite_cache_dir: root.join("overwrite-cache"),
  };
  let bytes = crate::core::format::ybn::xml::xml_to_ybn(include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/sample/ybn_conflicts/geometry_bvh.ybn.xml"
  )))
  .unwrap();
  for name in ["resource_a", "resource_b"] {
    let resource = pipeline.source_dir.join(name);
    fs::create_dir_all(resource.join("stream")).unwrap();
    fs::write(resource.join("fxmanifest.lua"), []).unwrap();
    fs::write(resource.join("stream/collision.ybn"), &bytes).unwrap();
  }
  fs::write(pipeline.source_dir.join("resource_a/stream/unique.ybn"), &bytes).unwrap();
  fs::write(pipeline.source_dir.join("resource_a/stream/custom.ytyp"), b"unmatched source")
    .unwrap();
  fs::create_dir_all(pipeline.vanilla_dir.join("0000-base/ybn")).unwrap();
  let mut changes = BTreeMap::new();
  for name in ["collision.ybn", "unique.ybn"] {
    let object = format!("0000-base/ybn/{name}");
    fs::write(pipeline.vanilla_dir.join(&object), &bytes).unwrap();
    changes.insert(
      name.into(),
      FileChange {
        previous_sha256: None,
        file: CachedFile {
          sha256: hex::encode(Sha256::digest(&bytes)),
          object,
          source: format!("base.rpf/{name}"),
        },
      },
    );
  }
  write_json(
    &pipeline.vanilla_dir.join("cache_info.json"),
    &VanillaCacheManifest {
      format_version: 1,
      game_dir: pipeline.game_dir.clone(),
      versions: vec![CacheVersion {
        id: "0000-base".into(),
        parent: None,
        archives: vec![],
        changes,
        unchanged: 0,
      }],
    },
  )
  .unwrap();
  let vanilla_manifest = fs::read(pipeline.vanilla_dir.join("cache_info.json")).unwrap();
  let first = pipeline.run().unwrap();
  assert_eq!(
    first.changes.vanilla_cache.added,
    ["latest/ybn/collision.ybn", "latest/ybn/unique.ybn"]
      .map(|path| pipeline.vanilla_cache_dir.join(path))
      .into()
  );
  assert_eq!(
    first.changes.source_cache.added,
    [
      "resources/resource_a/collision.ybn",
      "resources/resource_a/unique.ybn",
      "resources/resource_b/collision.ybn",
    ]
    .map(|path| pipeline.source_cache_dir.join(path))
    .into()
  );
  assert_eq!(first.changes.merged.added, [pipeline.merged_dir.join("ybn/collision.ybn")].into());
  assert_eq!(first.deploy.copied, 2);
  assert!(pipeline.vanilla_cache_dir.join("cache_info.json").is_file());
  assert!(pipeline.source_cache_dir.join("cache_info.json").is_file());
  assert!(pipeline.merged_dir.join("cache_info.json").is_file());
  for store in [&pipeline.vanilla_cache_dir, &pipeline.source_cache_dir, &pipeline.merged_dir] {
    assert!(store.join("stream_file_rtree.json").is_file(), "{}", store.display());
  }
  assert!(!pipeline.output_dir.join("stream/stream_file_rtree.json").exists());
  assert!(pipeline.merged_dir.join("ybn/collision.ybn").is_file());
  assert!(!pipeline.merged_dir.join("ybn/unique.ybn").exists());
  assert_eq!(
    fs::read(pipeline.output_dir.join("stream/ybn/merged/collision.ybn")).unwrap(),
    fs::read(pipeline.merged_dir.join("ybn/collision.ybn")).unwrap()
  );
  assert_eq!(
    fs::read(pipeline.output_dir.join("stream/ybn/clone/resource_a/unique.ybn")).unwrap(),
    bytes
  );
  assert!(pipeline.output_dir.join("files.txt").is_file());
  assert!(pipeline.source_dir.join("resource_a/stream/custom.ytyp").is_file());
  assert!(!pipeline.source_dir.join("resource_a/stream/collision.ybn").exists());
  assert_eq!(fs::read(pipeline.vanilla_dir.join("cache_info.json")).unwrap(), vanilla_manifest);
  let second = pipeline.run().unwrap();
  assert_eq!(second.deploy.copied, 0);
  assert_eq!(second.deploy.skipped, 2);
  assert_eq!(second.changes, PipelineChanges::default());
  // Every store matches its metadata, until a file is added or removed behind its back.
  let check = crate::core::check::Check {
    vanilla_dir: pipeline.vanilla_dir.clone(),
    vanilla_cache_dir: pipeline.vanilla_cache_dir.clone(),
    source_cache_dir: pipeline.source_cache_dir.clone(),
    overwrite_dir: pipeline.overwrite_dir.clone(),
    overwrite_cache_dir: pipeline.overwrite_cache_dir.clone(),
    merged_dir: pipeline.merged_dir.clone(),
    output_dir: pipeline.output_dir.clone(),
    deep: true,
  };
  assert_eq!(check.run().unwrap().issues, []);
  let stray = pipeline.source_cache_dir.join("resources/resource_a/stray.ybn");
  fs::write(&stray, b"stray").unwrap();
  let link = pipeline.vanilla_cache_dir.join("latest/ybn/unique.ybn");
  let aside = pipeline.vanilla_cache_dir.join("unique.ybn.aside");
  fs::rename(&link, &aside).unwrap();
  let kinds = |report: crate::core::check::CheckReport| {
    report.issues.into_iter().map(|issue| (issue.store, issue.kind, issue.path)).collect::<Vec<_>>()
  };
  use crate::core::check::CheckIssueKind::{Missing, Unrecorded};
  assert_eq!(
    kinds(check.run().unwrap()),
    [
      ("vanilla-cache".to_string(), Missing, "latest/ybn/unique.ybn".to_string()),
      ("vanilla-cache".to_string(), Unrecorded, "unique.ybn.aside".to_string()),
      ("source-cache".to_string(), Unrecorded, "resources/resource_a/stray.ybn".to_string()),
    ]
  );
  fs::remove_file(&stray).unwrap();
  fs::rename(&aside, &link).unwrap();

  // Moved elsewhere and named by other paths, every store is still current.
  let merge_metadata = fs::read(pipeline.merged_dir.join("cache_info.json")).unwrap();
  let moved_root = root.with_file_name(format!("default_pipeline_moved_{}", std::process::id()));
  let _ = fs::remove_dir_all(&moved_root);
  fs::rename(&root, &moved_root).unwrap();
  let moved = moved_root.join("unused/..");
  let relocated = Pipeline {
    source_dir: moved.join("input"),
    vanilla_dir: moved.join("vanilla"),
    vanilla_cache_dir: moved.join("vanilla-cache"),
    source_cache_dir: moved.join("source-cache"),
    merged_dir: moved.join("merged"),
    output_dir: moved.join("merged_mlo"),
    overwrite_cache_dir: moved.join("overwrite-cache"),
    ..pipeline.clone()
  };
  fs::create_dir_all(moved_root.join("unused")).unwrap();
  let after_move = relocated.run().unwrap();
  assert_eq!(after_move.changes, PipelineChanges::default());
  assert_eq!(after_move.deploy.copied, 0);
  assert_eq!(after_move.deploy.skipped, 2);
  assert_eq!(fs::read(relocated.merged_dir.join("cache_info.json")).unwrap(), merge_metadata);
  fs::remove_dir_all(moved_root.join("unused")).unwrap();
  fs::rename(&moved_root, &root).unwrap();
  let mut forced = pipeline.clone();
  forced.force = true;
  let forced = forced.run().unwrap();
  assert_eq!(forced.deploy.copied, 2);
  assert_eq!(forced.changes, PipelineChanges::default());
  assert_eq!(fs::read(pipeline.vanilla_dir.join("cache_info.json")).unwrap(), vanilla_manifest);
  let replacement = crate::core::format::ybn::xml::xml_to_ybn(include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/sample/ybn_conflicts/resource_a.ybn.xml"
  )))
  .unwrap();
  fs::write(pipeline.source_dir.join("resource_a/stream/collision.ybn"), &replacement).unwrap();
  fs::remove_dir_all(pipeline.source_dir.join("resource_b")).unwrap();
  let changed = pipeline.run().unwrap().changes;
  assert_eq!(changed.vanilla_cache, StoreChanges::default());
  assert_eq!(
    changed.source_cache,
    StoreChanges {
      added: Default::default(),
      modified: [pipeline.source_cache_dir.join("resources/resource_a/collision.ybn")].into(),
      removed: [pipeline.source_cache_dir.join("resources/resource_b/collision.ybn")].into(),
    }
  );
  assert_eq!(
    changed.merged,
    StoreChanges {
      added: Default::default(),
      modified: Default::default(),
      removed: [pipeline.merged_dir.join("ybn/collision.ybn")].into(),
    }
  );
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn pipeline_reuses_vanilla_and_applies_exclusions_only_for_this_run() {
  let root = std::env::temp_dir().join(format!("pipeline_generation_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let pipeline = Pipeline {
    source_dir: root.join("source"),
    game_dir: root.join("game"),
    magic: "magic.dat".into(),
    vanilla_dir: root.join("vanilla"),
    vanilla_cache_dir: root.join("vanilla-cache"),
    source_cache_dir: root.join("source-cache"),
    exclusions: Some(vec!["unused-resource".into()]),
    merged_dir: root.join("merged"),
    output_dir: root.join("output"),
    gamebuild: None,
    force: false,
    overwrite_dir: None,
    overwrite_cache_dir: root.join("overwrite-cache"),
  };
  fs::create_dir_all(pipeline.source_dir.join("resource")).unwrap();
  fs::write(pipeline.source_dir.join("resource/fxmanifest.lua"), []).unwrap();
  fs::create_dir_all(&pipeline.game_dir).unwrap();
  let excluded_resource = pipeline.source_dir.join("unused-resource");
  fs::create_dir_all(&excluded_resource).unwrap();
  fs::write(excluded_resource.join("fxmanifest.lua"), []).unwrap();
  fs::create_dir_all(&pipeline.vanilla_dir).unwrap();
  write_json(
    &pipeline.vanilla_dir.join("cache_info.json"),
    &VanillaCacheManifest {
      format_version: 1,
      game_dir: pipeline.game_dir.clone(),
      versions: vec![CacheVersion {
        id: "0000-base".into(),
        parent: None,
        archives: vec![],
        changes: BTreeMap::new(),
        unchanged: 0,
      }],
    },
  )
  .unwrap();
  pipeline.run().unwrap();
  assert!(pipeline.output_dir.join("deploy_cache_info.json").is_file());
  pipeline.run().unwrap();
  let exclusions_path = pipeline.source_cache_dir.join("source_exclusions.json");
  assert!(!exclusions_path.exists());
  let contains_excluded = || {
    let metadata: serde_json::Value = serde_json::from_reader(
      fs::File::open(pipeline.source_cache_dir.join("cache_info.json")).unwrap(),
    )
    .unwrap();
    metadata["resources"].get("unused-resource").is_some()
  };
  assert!(!contains_excluded());
  let mut unfiltered = pipeline.clone();
  unfiltered.exclusions = None;
  unfiltered.run().unwrap();
  assert!(contains_excluded());
  pipeline.run().unwrap();
  assert!(!contains_excluded());
  unfiltered.exclusions = Some(Vec::new());
  unfiltered.run().unwrap();
  assert!(contains_excluded());
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_vanilla_requires_a_game_directory_without_destroying_existing_data() {
  let root = std::env::temp_dir().join(format!("pipeline_missing_vanilla_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let pipeline = Pipeline {
    source_dir: root.join("input"),
    game_dir: root.join("missing-game"),
    magic: "magic.dat".into(),
    vanilla_dir: root.join("vanilla"),
    vanilla_cache_dir: root.join("vanilla-cache"),
    source_cache_dir: root.join("source-cache"),
    exclusions: None,
    merged_dir: root.join("merged"),
    output_dir: root.join("output"),
    gamebuild: None,
    force: false,
    overwrite_dir: None,
    overwrite_cache_dir: root.join("overwrite-cache"),
  };
  fs::create_dir_all(&pipeline.source_dir).unwrap();
  assert!(pipeline.run().unwrap_err().to_string().contains("specify --game-dir"));
  assert!(!pipeline.vanilla_cache_dir.exists());
  assert!(!pipeline.output_dir.exists());
  fs::create_dir_all(&pipeline.vanilla_dir).unwrap();
  fs::write(pipeline.vanilla_dir.join("keep.txt"), b"keep").unwrap();
  assert!(pipeline.run().unwrap_err().to_string().contains("refusing automatic replacement"));
  assert_eq!(fs::read(pipeline.vanilla_dir.join("keep.txt")).unwrap(), b"keep");
  let mut overlapping = pipeline.clone();
  overlapping.output_dir = pipeline.source_dir.clone();
  assert!(overlapping.validate_paths().is_ok());
  let mut nested = pipeline.clone();
  nested.source_dir = root.clone();
  assert!(nested.validate_paths().is_ok());
  assert!(!pipeline.vanilla_cache_dir.exists());
  let mut overlapping = pipeline.clone();
  overlapping.merged_dir = pipeline.source_cache_dir.join("nested");
  assert!(overlapping.run().unwrap_err().to_string().contains("Pipeline paths overlap"));
  assert!(!pipeline.output_dir.exists());
  fs::remove_dir_all(root).unwrap();
}
