use super::*;

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
  let original = signature(&connected).unwrap();
  let other = signature(&independent).unwrap();
  connected
    .get_mut("child.ymap")
    .unwrap()
    .vanilla
    .as_mut()
    .unwrap()
    .fingerprint
    .modified_seconds += 1;
  assert_eq!(signature(&connected).unwrap(), original);
  connected.get_mut("child.ymap").unwrap().vanilla.as_mut().unwrap().fingerprint.sha256 =
    "changed".into();
  assert_ne!(signature(&connected).unwrap(), original);
  assert_eq!(signature(&independent).unwrap(), other);
  connected.get_mut("child.ymap").unwrap().vanilla.as_mut().unwrap().fingerprint.sha256 =
    "child.ymap".into();
  connected.get_mut("child.ymap").unwrap().dependencies.clear();
  assert_ne!(signature(&connected).unwrap(), original);
}
