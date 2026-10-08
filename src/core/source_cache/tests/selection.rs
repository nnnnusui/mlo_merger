use super::*;

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
    cached_path: None,
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
      resource_checks: BTreeMap::new(),
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

  let inputs = crate::core::source_cache::load_merge_inputs(&cache_dir).unwrap();
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
      cached_path: None,
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
