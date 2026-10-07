use super::BuildSourceCache;
use super::{types::*, ymap_plan};
use crate::core::vanilla::{CacheVersion, VanillaCacheManifest, write_json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

#[test]
fn builds_source_inventory_and_skips_unchanged_inputs_without_diffing() {
  let root = std::env::temp_dir().join(format!("source_cache_test_{}", std::process::id()));
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
  assert_eq!(info.conflict_count, 2);
  assert_eq!(info.format_version, 2);
  let merge_inputs = super::load_merge_inputs(&output).unwrap();
  assert!(merge_inputs.ymap.is_empty());
  assert!(merge_inputs.ybn.is_empty());
  let vanilla_ymaps_to_read: BTreeSet<String> =
    serde_json::from_reader(fs::File::open(output.join("vanilla_ymaps_to_read.json")).unwrap())
      .unwrap();
  assert!(vanilla_ymaps_to_read.is_empty());
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

#[test]
fn source_cache_content_freshness_ignores_timestamp_only_changes() {
  let fingerprint = FileFingerprint {
    sha256: "same-content".into(),
    modified_seconds: 10,
    modified_nanos: 20,
    size: 30,
  };
  let cached = BTreeMap::from([("resource/stream/map.ymap".into(), fingerprint.clone())]);
  let mut touched = fingerprint;
  touched.modified_seconds += 1;
  touched.modified_nanos += 1;
  let current = BTreeMap::from([("resource/stream/map.ymap".into(), touched)]);

  assert!(super::publication::same_source_content(&cached, &current));
}

#[test]
fn source_cache_timestamp_refresh_updates_provenance_without_regeneration() {
  let root = std::env::temp_dir().join(format!("source_timestamp_refresh_{}", std::process::id()));
  fs::create_dir_all(&root).unwrap();
  let cached = FileFingerprint {
    sha256: "same-content".into(),
    modified_seconds: 10,
    modified_nanos: 20,
    size: 30,
  };
  let mut current = cached.clone();
  current.modified_seconds += 1;
  current.modified_nanos += 1;
  write_json(
    &root.join("source_cache_info.json"),
    &SourceCacheMetadata {
      format_version: 2,
      source_dir: root.join("source"),
      vanilla_dir: root.join("vanilla"),
      vanilla_cache_dir: root.join("vanilla-cache"),
      vanilla_manifest_sha256: "manifest".into(),
      vanilla_cache_revision: "derived".into(),
      latest_vanilla_version: "latest".into(),
      generated_at: "original-generation".into(),
      source_inputs: BTreeMap::from([("resource_a/stream/map.ymap".into(), cached)]),
      resources: BTreeMap::from([(
        "resource_a".into(),
        ResourceInventory {
          source: root.join("source/resource_a"),
          files_by_format: BTreeMap::from([(
            ".ymap".into(),
            vec![SourceFile {
              path: "stream/map.ymap".into(),
              file_name: "map.ymap".into(),
              sha256: "same-content".into(),
              size: 30,
              modified_seconds: 10,
              modified_nanos: 20,
              vanilla: None,
              ymap_parent_hash: None,
              metadata_error: None,
            }],
          )]),
        },
      )]),
      scanned_stream_file_count: 1,
      conflict_count: 0,
      ymap_load_plan: YmapLoadPlan {
        changed_source_parents: BTreeSet::new(),
        additional_source_children: BTreeSet::new(),
      },
      outputs: BTreeSet::new(),
    },
  )
  .unwrap();

  super::publication::refresh_source_timestamps(
    &root,
    &BTreeMap::from([("resource_a/stream/map.ymap".into(), current)]),
  )
  .unwrap();

  let refreshed: SourceCacheMetadata =
    serde_json::from_reader(fs::File::open(root.join("source_cache_info.json")).unwrap()).unwrap();
  assert_eq!(refreshed.generated_at, "original-generation");
  assert_eq!(refreshed.source_inputs["resource_a/stream/map.ymap"].modified_seconds, 11);
  assert_eq!(refreshed.resources["resource_a"].files_by_format[".ymap"][0].modified_seconds, 11);
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn merge_input_selector_uses_format_conflicts_and_ymap_load_closure() {
  use crate::core::stream_conflicts::{StreamConflictReport, StreamFileConflict};

  let root = std::env::temp_dir().join(format!("source_merge_inputs_{}", std::process::id()));
  let source_dir = root.join("source");
  let cache_dir = root.join("source-cache");
  let resource_a = source_dir.join("resource_a");
  let resource_b = source_dir.join("resource_b");
  fs::create_dir_all(resource_a.join("stream")).unwrap();
  fs::create_dir_all(resource_b.join("stream")).unwrap();
  for path in [
    resource_a.join("stream/parent.ymap"),
    resource_a.join("stream/unmatched.ymap"),
    resource_a.join("stream/collision.ybn"),
    resource_a.join("stream/unique.ybn"),
    resource_b.join("stream/child.ymap"),
    resource_b.join("stream/collision.ybn"),
  ] {
    fs::write(path, []).unwrap();
  }
  fs::create_dir_all(&cache_dir).unwrap();
  let source_dir = source_dir.canonicalize().unwrap();
  let resource_file = |path: &str, file_name: &str, vanilla: Option<VanillaMatch>| SourceFile {
    path: path.into(),
    file_name: file_name.into(),
    sha256: "source-hash".into(),
    size: 1,
    modified_seconds: 1,
    modified_nanos: 0,
    vanilla,
    ymap_parent_hash: None,
    metadata_error: None,
  };
  let matched = |content_matches| {
    Some(VanillaMatch {
      version: "0000-base".into(),
      sha256: "vanilla-hash".into(),
      content_matches,
    })
  };
  let source_ref = |resource: &str, source_path: &str, file_name: &str| SourceYmapRef {
    resource: resource.into(),
    source_path: source_path.into(),
    file_name: file_name.into(),
  };
  write_json(
    &cache_dir.join("source_cache_info.json"),
    &SourceCacheMetadata {
      format_version: 2,
      source_dir: source_dir.clone(),
      vanilla_dir: root.join("vanilla"),
      vanilla_cache_dir: root.join("vanilla-cache"),
      vanilla_manifest_sha256: "manifest".into(),
      vanilla_cache_revision: "derived".into(),
      latest_vanilla_version: "0000-base".into(),
      generated_at: "now".into(),
      source_inputs: BTreeMap::new(),
      resources: BTreeMap::from([
        (
          "resource_a".into(),
          ResourceInventory {
            source: source_dir.join("resource_a"),
            files_by_format: BTreeMap::from([
              (
                ".ymap".into(),
                vec![
                  resource_file("stream/parent.ymap", "parent.ymap", matched(false)),
                  resource_file("stream/unmatched.ymap", "unmatched.ymap", None),
                ],
              ),
              (
                ".ybn".into(),
                vec![
                  resource_file("stream/collision.ybn", "collision.ybn", matched(true)),
                  resource_file("stream/unique.ybn", "unique.ybn", matched(false)),
                ],
              ),
            ]),
          },
        ),
        (
          "resource_b".into(),
          ResourceInventory {
            source: source_dir.join("resource_b"),
            files_by_format: BTreeMap::from([
              (
                ".ymap".into(),
                vec![resource_file("stream/child.ymap", "child.ymap", matched(true))],
              ),
              (
                ".ybn".into(),
                vec![resource_file("stream/collision.ybn", "collision.ybn", matched(true))],
              ),
            ]),
          },
        ),
      ]),
      scanned_stream_file_count: 6,
      conflict_count: 1,
      ymap_load_plan: YmapLoadPlan {
        changed_source_parents: BTreeSet::from([source_ref(
          "resource_a",
          "stream/parent.ymap",
          "parent.ymap",
        )]),
        additional_source_children: BTreeSet::from([source_ref(
          "resource_b",
          "stream/child.ymap",
          "child.ymap",
        )]),
      },
      outputs: BTreeSet::new(),
    },
  )
  .unwrap();
  write_json(
    &cache_dir.join("stream_conflicts.json"),
    &StreamConflictReport {
      input_dir: source_dir.to_string_lossy().into_owned(),
      scanned_file_count: 6,
      conflict_count: 1,
      conflicts: BTreeMap::from([(
        ".ybn".into(),
        vec![StreamFileConflict {
          file_name: "collision.ybn".into(),
          paths: vec![
            "resource_a/stream/collision.ybn".into(),
            "resource_b/stream/collision.ybn".into(),
          ],
        }],
      )]),
    },
  )
  .unwrap();
  write_json(
    &cache_dir.join("vanilla_ymaps_to_read.json"),
    &BTreeSet::from(["parent.ymap".to_owned(), "child.ymap".to_owned()]),
  )
  .unwrap();

  let inputs = super::load_merge_inputs(&cache_dir).unwrap();
  assert_eq!(
    inputs.ymap.iter().map(|source| source.file_name.as_str()).collect::<Vec<_>>(),
    ["parent.ymap", "child.ymap"]
  );
  assert_eq!(inputs.ybn.len(), 2);
  assert!(inputs.ybn.iter().all(|source| source.file_name == "collision.ybn"));
  assert_eq!(
    inputs.vanilla_ymaps_to_read,
    BTreeSet::from(["parent.ymap".into(), "child.ymap".into()])
  );
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn parent_change_lists_vanilla_closure_and_source_children_in_other_resources() {
  let parent_name = "parent_map.ymap";
  let child_name = "child_map.ymap";
  let parent_hash = crate::core::format::ymap::diff::reference_hash("parent_map");
  let source_file =
    |file_name: &str, vanilla: Option<VanillaMatch>, parent: Option<String>| SourceFile {
      path: format!("stream/{file_name}"),
      file_name: file_name.into(),
      sha256: "source-hash".into(),
      size: 10,
      modified_seconds: 1,
      modified_nanos: 0,
      vanilla,
      ymap_parent_hash: parent,
      metadata_error: None,
    };
  let resources = BTreeMap::from([
    (
      "resource_parent".into(),
      ResourceInventory {
        source: "resource_parent".into(),
        files_by_format: BTreeMap::from([(
          ".ymap".into(),
          vec![source_file(
            parent_name,
            Some(VanillaMatch {
              version: "0000-base".into(),
              sha256: "old-parent-hash".into(),
              content_matches: false,
            }),
            None,
          )],
        )]),
      },
    ),
    (
      "resource_child".into(),
      ResourceInventory {
        source: "resource_child".into(),
        files_by_format: BTreeMap::from([(
          ".ymap".into(),
          vec![source_file(
            child_name,
            Some(VanillaMatch {
              version: "0000-base".into(),
              sha256: "vanilla-hash".into(),
              content_matches: true,
            }),
            Some(format!("{parent_hash:08X}")),
          )],
        )]),
      },
    ),
  ]);
  let relationships = YmapRelationshipIndex {
    format_version: 2,
    vanilla_manifest_sha256: "revision".into(),
    version: "latest".into(),
    children_by_parent_hash: BTreeMap::from([(
      format!("{parent_hash:08x}"),
      vec![child_name.into(), "vanilla_only_child.ymap".into()],
    )]),
  };
  let vanilla_files = BTreeMap::from([
    (
      parent_name.into(),
      DerivedVanillaFile {
        version: "0000-base".into(),
        sha256: "parent".into(),
        object: "latest/ymap/parent_map.ymap".into(),
      },
    ),
    (
      child_name.into(),
      DerivedVanillaFile {
        version: "0000-base".into(),
        sha256: "child".into(),
        object: "latest/ymap/child_map.ymap".into(),
      },
    ),
    (
      "vanilla_only_child.ymap".into(),
      DerivedVanillaFile {
        version: "0000-base".into(),
        sha256: "vanilla-child".into(),
        object: "latest/ymap/vanilla_only_child.ymap".into(),
      },
    ),
  ]);

  let plan = ymap_plan::build(&resources);
  assert!(plan.changed_source_parents.contains(&SourceYmapRef {
    resource: "resource_parent".into(),
    source_path: "stream/parent_map.ymap".into(),
    file_name: parent_name.into(),
  }));
  assert!(plan.additional_source_children.contains(&SourceYmapRef {
    resource: "resource_child".into(),
    source_path: "stream/child_map.ymap".into(),
    file_name: child_name.into(),
  }));
  let vanilla_ymaps_to_read =
    ymap_plan::vanilla_ymaps_to_read(&plan.changed_source_parents, &relationships, &vanilla_files);
  assert!(vanilla_ymaps_to_read.contains(parent_name));
  assert!(vanilla_ymaps_to_read.contains(child_name));
  assert!(vanilla_ymaps_to_read.contains("vanilla_only_child.ymap"));
}
