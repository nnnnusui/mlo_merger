use super::*;
use crate::platform::fs;
use std::collections::BTreeSet;

/// An empty directory for one test.
fn directory(name: &str) -> PathBuf {
  let root = std::env::temp_dir().join(format!("check_{name}_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  fs::create_dir_all(&root).unwrap();
  root
}

fn write(
  path: &Path,
  bytes: &[u8],
) {
  fs::create_dir_all(path.parent().unwrap()).unwrap();
  fs::write(path, bytes).unwrap();
}

/// The issues as `(kind, path, detail)`, which the store name does not change.
fn issues(findings: Findings) -> Vec<(CheckIssueKind, String, Option<String>)> {
  findings.into_issues().into_iter().map(|issue| (issue.kind, issue.path, issue.detail)).collect()
}

fn sha256(bytes: &[u8]) -> String {
  use sha2::{Digest, Sha256};
  hex::encode(Sha256::digest(bytes))
}

#[test]
fn files_below_lists_files_and_links_relative_to_the_root() {
  let root = directory("files_below");
  write(&root.join("store/a.ymap"), b"a");
  write(&root.join("store/nested/b.ybn"), b"b");
  write(&root.join("other/c.ymap"), b"c");
  #[cfg(unix)]
  std::os::unix::fs::symlink(root.join("other"), root.join("store/link")).unwrap();
  let mut files = files_below(&root, "store").unwrap();
  files.sort();
  let mut expected = vec!["store/a.ymap", "store/nested/b.ybn"];
  // A link is listed itself, not followed.
  if cfg!(unix) {
    expected.push("store/link");
  }
  expected.sort();
  assert_eq!(files, expected);
  assert!(files_below(&root, "missing").unwrap().is_empty());
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn recorded_files_are_compared_by_size_and_with_deep_by_contents() {
  let root = directory("recorded_file");
  write(&root.join("same.ymap"), b"same");
  write(&root.join("longer.ymap"), b"longer contents");
  write(&root.join("edited.ymap"), b"edit");
  let recorded = sha256(b"same");
  let check = |deep: bool| {
    let mut findings = Findings::new("store");
    for (path, size) in [
      ("same.ymap", Some(4)),
      ("longer.ymap", Some(4)),
      ("edited.ymap", Some(4)),
      ("gone.ymap", None),
    ] {
      findings.recorded_file(&root, path, size, &recorded, deep).unwrap();
    }
    issues(findings)
  };
  let size_only = vec![
    (CheckIssueKind::Changed, "longer.ymap".into(), Some("size 15 instead of 4".into())),
    (CheckIssueKind::Missing, "gone.ymap".into(), None),
  ];
  // Without `deep`, a file of the recorded size is taken as recorded.
  assert_eq!(check(false), size_only);
  let mut deep = size_only;
  deep.insert(
    1,
    (CheckIssueKind::Changed, "edited.ymap".into(), Some(format!("SHA-256 {}", sha256(b"edit")))),
  );
  assert_eq!(check(true), deep);
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn files_and_entries_outside_the_record_are_unrecorded() {
  let root = directory("unrecorded");
  write(&root.join("resources/a/kept.ybn"), b"");
  write(&root.join("resources/a/stray.ybn"), b"");
  write(&root.join("cache_info.json"), b"");
  write(&root.join(".staging-1/leftover"), b"");
  let mut findings = Findings::new("store");
  let recorded = BTreeSet::from(["resources/a/kept.ybn".to_string()]);
  findings.unrecorded_below(&root, "resources", &recorded).unwrap();
  findings.unrecorded_below(&root, "missing", &recorded).unwrap();
  findings.unknown_entries(&root, &["cache_info.json", "resources"]).unwrap();
  assert_eq!(
    issues(findings),
    [
      (CheckIssueKind::Unrecorded, "resources/a/stray.ybn".into(), None),
      (CheckIssueKind::Unrecorded, ".staging-1".into(), None),
    ]
  );
  fs::remove_dir_all(root).unwrap();
}

fn stores(root: &Path) -> Check {
  Check {
    vanilla_dir: root.join("vanilla"),
    vanilla_cache_dir: root.join("vanilla-cache"),
    source_cache_dir: root.join("source-cache"),
    overwrite_dir: None,
    overwrite_cache_dir: root.join("overwrite-cache"),
    merged_dir: root.join("merged"),
    output_dir: root.join("merged_mlo"),
    deep: false,
  }
}

#[test]
fn missing_stores_are_skipped() {
  let root = directory("missing_stores");
  assert_eq!(stores(&root).run().unwrap(), CheckReport::default());
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn each_store_whose_metadata_cannot_be_read_is_reported_once() {
  let root = directory("unreadable");
  let check = stores(&root);
  for metadata in [
    check.vanilla_dir.join("cache_info.json"),
    check.vanilla_cache_dir.join("cache_info.json"),
    check.source_cache_dir.join("cache_info.json"),
    check.overwrite_cache_dir.join("added_stream_files.json"),
    check.merged_dir.join("cache_info.json"),
    check.output_dir.join("deploy_cache_info.json"),
  ] {
    write(&metadata, b"not JSON");
  }
  // Stray files are not looked for once the metadata cannot be read.
  write(&check.source_cache_dir.join("resources/a/stray.ybn"), b"");
  let report = check.run().unwrap();
  assert_eq!(
    report
      .issues
      .iter()
      .map(|issue| (issue.store.as_str(), issue.kind, issue.path.as_str()))
      .collect::<Vec<_>>(),
    [
      ("vanilla", CheckIssueKind::Unreadable, "cache_info.json"),
      ("vanilla-cache", CheckIssueKind::Unreadable, "cache_info.json"),
      ("source-cache", CheckIssueKind::Unreadable, "cache_info.json"),
      ("overwrite-cache", CheckIssueKind::Unreadable, "added_stream_files.json"),
      ("merged", CheckIssueKind::Unreadable, "cache_info.json"),
      ("deployment", CheckIssueKind::Unreadable, "deploy_cache_info.json"),
    ]
  );
  assert!(report.issues.iter().all(|issue| issue.detail.is_some()));
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn issues_serialize_with_snake_case_kinds_and_without_empty_details() {
  let issue = |detail: Option<&str>| CheckIssue {
    store: "merged".into(),
    kind: CheckIssueKind::Changed,
    path: "ymap/map.ymap".into(),
    detail: detail.map(Into::into),
  };
  let report = CheckReport {
    issues: vec![issue(None), issue(Some("size 1 instead of 2"))],
  };
  assert_eq!(
    serde_json::to_value(&report).unwrap(),
    serde_json::json!({"issues": [
      {"store": "merged", "kind": "changed", "path": "ymap/map.ymap"},
      {"store": "merged", "kind": "changed", "path": "ymap/map.ymap",
       "detail": "size 1 instead of 2"},
    ]})
  );
}
