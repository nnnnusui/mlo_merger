use super::*;
use crate::common::fingerprint::Fingerprint;
use crate::core::{
  format::ymap::compare::YmapComparisonDiff,
  source_cache::generate::diff::{YmapDiffCounts, write_ymap_diffs},
};
use std::path::{Path, PathBuf};

/// A source cache whose one source YMAP overrides a vanilla YMAP of the same name.
///
/// Neither YMAP can be decoded, so a diff is only ever kept, never generated.
struct Fixture {
  root: PathBuf,
  output: PathBuf,
  source_ymap: PathBuf,
  vanilla_ymap: PathBuf,
}

impl Fixture {
  fn new(name: &str) -> Self {
    let root = std::env::temp_dir().join(format!("{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let output = root.join("source-cache");
    let source = root.join("source/resource_a");
    let vanilla_cache = root.join("vanilla-cache");
    fs::create_dir_all(source.join("stream")).unwrap();
    fs::create_dir_all(vanilla_cache.join("latest/ymap")).unwrap();
    fs::create_dir_all(&output).unwrap();
    let source_ymap = source.join("stream/map.ymap");
    let vanilla_ymap = vanilla_cache.join("latest/ymap/map.ymap");
    fs::write(&source_ymap, b"source bytes").unwrap();
    fs::write(&vanilla_ymap, b"vanilla bytes").unwrap();
    write_json(
      &vanilla_cache.join("cache_info.json"),
      &serde_json::json!({ "latest_version": "v1" }),
    )
    .unwrap();
    write_json(
      &output.join("source_cache_info.json"),
      &SourceCacheMetadata {
        format_version: 2,
        source_dir: root.join("source"),
        vanilla_dir: root.join("vanilla"),
        vanilla_cache_dir: vanilla_cache,
        vanilla_manifest_sha256: "manifest".into(),
        vanilla_cache_revision: "derived".into(),
        latest_vanilla_version: "v1".into(),
        generated_at: "generation".into(),
        source_inputs: BTreeMap::new(),
        resource_checks: BTreeMap::new(),
        resources: BTreeMap::from([(
          "resource_a".into(),
          ResourceInventory {
            source,
            files_by_format: BTreeMap::from([(
              ".ymap".into(),
              vec![SourceFile {
                path: "stream/map.ymap".into(),
                file_name: "map.ymap".into(),
                sha256: "source".into(),
                size: 12,
                modified_seconds: 0,
                modified_nanos: 0,
                vanilla: Some(VanillaMatch {
                  version: "v1".into(),
                  sha256: "vanilla".into(),
                  content_matches: false,
                }),
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
    Self {
      root,
      output,
      source_ymap,
      vanilla_ymap,
    }
  }

  fn diff_path(&self) -> PathBuf {
    self.output.join("diff/resource_a/map.ymap.diff.json")
  }

  /// Writes a diff document recording the current fingerprints of both YMAPs.
  fn write_current_diff(&self) -> serde_json::Value {
    self.write_diff(YmapComparisonDiff::SCHEMA_VERSION)
  }

  /// Writes a diff document of `schema_version` recording the current fingerprints of both
  /// YMAPs.
  fn write_diff(
    &self,
    schema_version: u32,
  ) -> serde_json::Value {
    let file = |path: &Path| {
      let fingerprint = Fingerprint::read(path, None, false).unwrap();
      serde_json::json!({
        "path": path.display().to_string(),
        "sha256": fingerprint.sha256,
        "size": fingerprint.size,
        "modified_seconds": fingerprint.modified_seconds,
        "modified_nanos": fingerprint.modified_nanos,
      })
    };
    let document = serde_json::json!({
      "schema_version": schema_version,
      "source": file(&self.source_ymap),
      "vanilla": file(&self.vanilla_ymap.canonicalize().unwrap()),
      "diffs": { "name": "kept" },
    });
    fs::create_dir_all(self.diff_path().parent().unwrap()).unwrap();
    write_json(&self.diff_path(), &document).unwrap();
    document
  }

  fn read_diff(&self) -> serde_json::Value {
    serde_json::from_slice(&fs::read(self.diff_path()).unwrap()).unwrap()
  }

  fn write_diffs(&self) -> YmapDiffCounts {
    write_ymap_diffs(&self.output, &self.root.join("vanilla"), &self.root.join("vanilla-cache"))
      .unwrap()
  }
}

impl Drop for Fixture {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.root);
  }
}

const KEPT: YmapDiffCounts = YmapDiffCounts {
  written: 0,
  reused: 1,
};
const DROPPED: YmapDiffCounts = YmapDiffCounts {
  written: 0,
  reused: 0,
};

#[test]
fn ymap_diffs_are_kept_while_their_ymaps_are_unchanged() {
  let fixture = Fixture::new("ymap_diffs_kept");
  let document = fixture.write_current_diff();
  assert_eq!(fixture.write_diffs(), KEPT);
  assert_eq!(fixture.read_diff(), document);
  assert_eq!(fixture.write_diffs(), KEPT);
  assert_eq!(fixture.read_diff(), document);
  assert!(!fixture.output.join("diff.tmp").exists());
}

#[test]
fn ymap_diffs_are_regenerated_when_the_source_ymap_changes() {
  let fixture = Fixture::new("ymap_diffs_source_changed");
  fixture.write_current_diff();
  fs::write(&fixture.source_ymap, b"edited source bytes").unwrap();
  // The edited YMAP cannot be decoded, so regenerating it drops the stale diff.
  assert_eq!(fixture.write_diffs(), DROPPED);
  assert!(!fixture.diff_path().exists());
}

#[test]
fn ymap_diffs_are_regenerated_when_the_vanilla_ymap_changes() {
  let fixture = Fixture::new("ymap_diffs_vanilla_changed");
  fixture.write_current_diff();
  fs::write(&fixture.vanilla_ymap, b"updated vanilla bytes").unwrap();
  assert_eq!(fixture.write_diffs(), DROPPED);
  assert!(!fixture.diff_path().exists());
}

#[test]
fn ymap_diffs_are_regenerated_when_only_the_modification_time_changes() {
  let fixture = Fixture::new("ymap_diffs_touched");
  fixture.write_current_diff();
  let file = fs::File::options().write(true).open(&fixture.source_ymap).unwrap();
  file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1)).unwrap();
  assert_eq!(fixture.write_diffs(), DROPPED);
}

#[test]
fn unreadable_ymap_diffs_are_regenerated() {
  let fixture = Fixture::new("ymap_diffs_unreadable");
  fs::create_dir_all(fixture.diff_path().parent().unwrap()).unwrap();
  fs::write(fixture.diff_path(), b"not JSON").unwrap();
  assert_eq!(fixture.write_diffs(), DROPPED);
  assert!(!fixture.diff_path().exists());
}

#[test]
fn ymap_diffs_of_another_schema_version_are_regenerated() {
  let fixture = Fixture::new("ymap_diffs_schema_changed");
  fixture.write_diff(YmapComparisonDiff::SCHEMA_VERSION - 1);
  assert_eq!(fixture.write_diffs(), DROPPED);
  assert!(!fixture.diff_path().exists());
}
