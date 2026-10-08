use super::*;

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
      resource_checks: BTreeMap::new(),
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
              cached_path: None,
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

  crate::core::source_cache::publication::refresh_source_timestamps(
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
