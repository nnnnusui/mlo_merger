use super::*;

#[test]
fn ybn_version_change_reuses_current_ymap_groups() {
  let root = std::env::temp_dir().join(format!("merge_type_versions_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let source = root.join("source");
  let cache = root.join("cache");
  let vanilla = root.join("vanilla");
  let output = root.join("merged");
  let staging = root.join("staging");
  for path in [&source, &cache, &vanilla, &output, &staging] {
    fs::create_dir_all(path).unwrap();
  }
  write_json(
    &vanilla.join("ymap_relationships.json"),
    &serde_json::json!({
      "children_by_parent": {}
    }),
  )
  .unwrap();
  write_json(
    &cache.join("source_cache_info.json"),
    &serde_json::json!({
      "resources": {}
    }),
  )
  .unwrap();
  let make_source = |extension: &str| {
    let name = format!("map.{extension}");
    let path = cache.join(&name);
    fs::write(&path, b"unchanged input").unwrap();
    MergeSourceFile {
      resource: "resource".into(),
      path,
      original_path: source.join(&name),
      file_name: name,
    }
  };
  let ymap = make_source("ymap");
  let ybn = make_source("ybn");
  let mut records = BTreeMap::new();
  for (extension, input) in [("ymap", &ymap), ("ybn", &ybn)] {
    let mut record = FileRecord {
      merge_sources: vec![InputFile {
        path: input.path.clone(),
        resource: Some(input.resource.clone()),
        original_path: Some(input.original_path.clone()),
        fingerprint: Fingerprint::read(&input.path, None, false).unwrap(),
      }],
      vanilla: None,
      dependencies: BTreeSet::new(),
      dependency_fingerprint: String::new(),
      output: None,
      merged_at: "cached".into(),
      duplicates: Vec::new(),
    };
    record.dependency_fingerprint =
      signature(extension, &BTreeMap::from([(input.file_name.clone(), record.clone())])).unwrap();
    records.insert(input.file_name.clone(), record);
  }
  let mut versions = algorithm_versions();
  versions.insert("ybn".into(), YBN_ALGORITHM_VERSION - 1);
  write_json(
    &output.join("merge_cache_info.json"),
    &MergeMetadata {
      format_version: 1,
      algorithm_version: None,
      algorithm_versions: versions,
      generated_at: "cached".into(),
      files: records,
    },
  )
  .unwrap();
  let inputs = MergeInputs {
    ymap: vec![ymap],
    ybn: vec![ybn],
    vanilla_ymaps_to_read: BTreeSet::new(),
  };
  assert!(
    IncrementalMerge {
      source_dir: &source,
      vanilla_cache: &vanilla,
      source_cache: &cache,
      output: &output,
      staging: &staging,
      ybn: &[],
      ymap: &[],
      inputs: &inputs,
      force: false,
    }
    .run()
    .unwrap()
  );
  let result = MergeMetadata::load(&staging).unwrap().unwrap();
  assert_eq!(result.files["map.ymap"].merged_at, "cached");
  assert_ne!(result.files["map.ybn"].merged_at, "cached");
  assert_eq!(result.algorithm_versions, algorithm_versions());
  let json: serde_json::Value =
    serde_json::from_slice(&fs::read(staging.join("merge_cache_info.json")).unwrap()).unwrap();
  assert!(json.get("algorithm_version").is_none());
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn dependency_groups_invalidate_connected_maps_but_not_independent_maps() {
  let graph = BTreeMap::from([
    ("parent.ymap".into(), BTreeSet::from(["child.ymap".into()])),
    ("child.ymap".into(), BTreeSet::from(["parent.ymap".into()])),
    ("independent.ymap".into(), BTreeSet::new()),
  ]);
  let source = |name: &str| MergeSourceFile {
    resource: "resource".into(),
    path: PathBuf::from(name),
    original_path: PathBuf::from(name),
    file_name: name.into(),
  };
  let parent = source("parent.ymap");
  let independent = source("independent.ymap");
  let sources = BTreeMap::from([
    ("parent.ymap".into(), vec![&parent]),
    ("independent.ymap".into(), vec![&independent]),
  ]);
  let groups = groups(&sources, &graph);
  assert_eq!(groups.len(), 2);
  assert_eq!(
    groups.iter().find(|group| group.names.contains("parent.ymap")).unwrap().names,
    BTreeSet::from(["parent.ymap".into(), "child.ymap".into()])
  );
  let record = |name: &str| FileRecord {
    merge_sources: vec![],
    vanilla: Some(InputFile {
      path: PathBuf::from(name),
      resource: None,
      original_path: None,
      fingerprint: Fingerprint {
        sha256: name.into(),
        size: 10,
        modified_seconds: 20,
        modified_nanos: 0,
      },
    }),
    dependencies: graph[name].clone(),
    dependency_fingerprint: String::new(),
    output: None,
    merged_at: "cached".into(),
    duplicates: Vec::new(),
  };
  let mut connected = BTreeMap::from([
    ("parent.ymap".into(), record("parent.ymap")),
    ("child.ymap".into(), record("child.ymap")),
  ]);
  let independent = BTreeMap::from([("independent.ymap".into(), record("independent.ymap"))]);
  let original = signature("ymap", &connected).unwrap();
  let other = signature("ymap", &independent).unwrap();
  connected
    .get_mut("child.ymap")
    .unwrap()
    .vanilla
    .as_mut()
    .unwrap()
    .fingerprint
    .modified_seconds += 1;
  assert_eq!(signature("ymap", &connected).unwrap(), original);
  connected.get_mut("child.ymap").unwrap().vanilla.as_mut().unwrap().fingerprint.sha256 =
    "changed".into();
  assert_ne!(signature("ymap", &connected).unwrap(), original);
  assert_eq!(signature("ymap", &independent).unwrap(), other);
  connected.get_mut("child.ymap").unwrap().vanilla.as_mut().unwrap().fingerprint.sha256 =
    "child.ymap".into();
  connected.get_mut("child.ymap").unwrap().dependencies.clear();
  assert_ne!(signature("ymap", &connected).unwrap(), original);
}
