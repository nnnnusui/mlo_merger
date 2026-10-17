//! Generates TypeScript types from the same Rust models used by Serde.

use specta::ts::{BigIntExportBehavior, ExportConfiguration};
use std::{fs, path::Path};

/// Exports report, cache and configuration types without running the asset pipeline.
///
/// JSON serializes file sizes and timestamps as numbers, so their TypeScript
/// representation is `number`, not `bigint`. Consumers must account for precision
/// loss above `Number.MAX_SAFE_INTEGER`; type generation does not validate values.
///
/// ```no_run
/// mlo_merger::typescript::export_types(std::path::Path::new("asset/gen_mlo_merger.ts"))?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn export_types(output: &Path) -> Result<(), Box<dyn std::error::Error>> {
  let path = output.to_str().ok_or("TypeScript output path must be UTF-8")?;
  if let Some(parent) = output.parent().filter(|parent| !parent.as_os_str().is_empty()) {
    fs::create_dir_all(parent)?;
  }
  specta::export::ts_with_cfg(
    path,
    &ExportConfiguration::new().bigint(BigIntExportBehavior::Number),
  )?;
  let mut file = fs::OpenOptions::new().append(true).open(output)?;
  std::io::Write::write_all(&mut file, crate::core::overwrite::YMAP_DIFF_SKIP_TS.as_bytes())?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::stream_conflicts::{StreamConflictReport, StreamFileConflict};
  use std::collections::BTreeMap;

  #[test]
  fn generated_types_are_deterministic_and_cover_serialized_contracts() {
    let path = std::env::temp_dir().join(format!("mlo_typescript_{}.ts", std::process::id()));
    export_types(&path).unwrap();
    let actual = fs::read_to_string(&path).unwrap();
    export_types(&path).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), actual);
    for name in [
      "DuplicateReport",
      "MergeMetadata",
      "SourceCacheMetadata",
      "SourceCacheConflictReport",
      "DerivedManifest",
      "VanillaCacheManifest",
      "DeployMetadata",
      "GenerationReport",
      "ResourceReport",
      "MatchingConfig",
      "BlacklistConfig",
      "RTreeIndex",
      "RTreeItem",
      "YmapComparisonDiff",
      "YmapComparisonFile",
      "YmapDiffFields",
      "YmapEntityDiff",
      "YmapEntityFieldDiff",
      "ResourceOverwrite",
      "StreamFileOverwrite",
      "OverwriteFingerprint",
      "YmapDiffSkip",
    ] {
      assert!(actual.contains(&format!("export type {name} =")), "Missing {name}");
    }
    assert!(actual.contains("schemaVersion: number"));
    assert!(actual.contains("mergedFileCount: number"));
    assert!(actual.contains("minX: number"));
    assert!(actual.contains("originalPath?: string"));
    assert!(actual.contains("export type Change = \"Added\" | \"Modified\" | \"Removed\""));
    assert!(actual.contains("bmin?: [number, number, number] | null"));
    assert!(actual.contains("unsupported_files?: string[]"));
    assert!(actual.contains("tolerance?: MatchTolerances"));
    assert!(actual.contains("source_conflicts?:"));
    assert!(actual.contains("modified_seconds: number"));
    assert!(actual.contains("algorithm_versions?: { [key: string]: number }"));
    assert!(actual.contains("entities?: YmapEntityDiff[]"));
    assert!(actual.contains("skip: YmapDiffSkip"));
    assert!(!actual.contains(": bigint"));
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn stream_conflict_wire_names_remain_camel_case() {
    let report = StreamConflictReport {
      input_dir: "cache".into(),
      scanned_file_count: 2,
      conflict_count: 1,
      conflicts: BTreeMap::from([(
        ".ybn".into(),
        vec![StreamFileConflict {
          file_name: "collision.ybn".into(),
          paths: vec!["a/collision.ybn".into(), "b/collision.ybn".into()],
        }],
      )]),
    };
    let json = serde_json::to_value(report).unwrap();
    assert_eq!(json["inputDir"], "cache");
    assert_eq!(json["scannedFileCount"], 2);
    assert_eq!(json["conflicts"][".ybn"][0]["fileName"], "collision.ybn");
    assert!(json.get("input_dir").is_none());
  }
}
