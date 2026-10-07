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
  assert_eq!(info.conflict_count, 1);
  assert_eq!(info.format_version, 2);
  let vanilla_ymaps_to_read: BTreeSet<String> =
    serde_json::from_reader(fs::File::open(output.join("vanilla_ymaps_to_read.json")).unwrap())
      .unwrap();
  assert!(vanilla_ymaps_to_read.is_empty());
  assert!(!build().run().unwrap());

  fs::remove_file(output.join("vanilla_ymaps_to_read.json")).unwrap();
  assert!(build().run().unwrap());
  assert!(!build().run().unwrap());

  fs::write(source_resource.join("stream/unmatched.ymap"), b"changed bytes").unwrap();
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
