//! Generates TypeScript types from the same Rust models used by Serde.

use crate::fs;
use specta::ts::{BigIntExportBehavior, ExportConfiguration};
use std::path::Path;

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
  // Specta 1 makes every field of an enum variant required, but `ItemDiff::Removed` leaves out
  // `implied` when it is false, and the removals and modifications of `ItemDiff` and
  // `YmapEntityDiff` leave out `reason` when it is `None`.
  let mut types = fs::read_to_string(output)?;
  for (required, optional, count) in [
    (
      "{ Removed: { index: number; implied: boolean; reason: string | null } }",
      "{ Removed: { index: number; implied?: boolean; reason?: string | null } }",
      1,
    ),
    ("; reason: string | null } }", "; reason?: string | null } }", 3),
  ] {
    if types.matches(required).count() != count {
      return Err(format!("Generated diff types no longer spell `{required}` as expected").into());
    }
    types = types.replace(required, optional);
  }
  fs::write(output, types)?;
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
      "MatchingConfig",
      "RTreeIndex",
      "RTreeItem",
      "YmapComparisonDiff",
      "YmapComparisonFile",
      "YmapPropagatedDiff",
      "YmapPropagationOrigin",
      "GenerationIndex",
      "GenerationUnit",
      "VanillaYmapDiff",
      "VanillaYmapVersion",
      "VanillaYmapVersionIndex",
      "YmapDiffFields",
      "YmapEntityDiff",
      "YmapEntityFieldDiff",
      "ResourceOverwrite",
      "StreamFileOverwrite",
      "OverwriteFingerprint",
      "VanillaOverwrite",
      "YmapDiffSkip",
      "PipelineChanges",
      "PipelineReport",
      "CheckReport",
      "CheckIssue",
      "CheckIssueKind",
      "IgnoredOverwrite",
      "IgnoredOverwriteReason",
      "StoreChanges",
      "ReadResult",
      "ReadFileResult",
      "ReadOrigin",
      "YmapReadDocument",
      "ReadFingerprint",
      "Ymap",
    ] {
      assert!(actual.contains(&format!("export type {name} =")), "Missing {name}");
    }
    assert!(actual.contains("schemaVersion: number"));
    assert!(actual.contains("fileCount: number"));
    assert!(actual.contains("ymap: RTreeItem[]"));
    assert!(actual.contains("minX: number"));
    assert!(actual.contains("originalPath?: string"));
    assert!(
      actual.contains("{ Removed: { index: number; implied?: boolean; reason?: string | null } }")
    );
    assert!(actual.contains("{ Removed: { index: number; reason?: string | null } }"));
    assert!(!actual.contains("reason: string | null"));
    assert!(actual.contains("partner?: YmapComparisonPartner | null"));
    assert!(actual.contains("export type Change = \"Added\" | \"Modified\" | \"Removed\""));
    assert!(actual.contains("tolerance?: MatchTolerances"));
    assert!(actual.contains("source_conflicts?:"));
    assert!(actual.contains("modified_seconds: number"));
    assert!(actual.contains("algorithm_versions?: { [key: string]: number }"));
    assert!(actual.contains("entities?: YmapEntityDiff[]"));
    assert!(actual.contains("skip: YmapDiffSkip"));
    assert!(actual.contains("vanilla_cache: StoreChanges"));
    assert!(actual.contains("added: string[]"));
    assert!(!actual.contains(": bigint"));
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn stream_conflict_wire_names_remain_camel_case() {
    let report = StreamConflictReport {
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
    assert_eq!(json["scannedFileCount"], 2);
    assert_eq!(json["conflicts"][".ybn"][0]["fileName"], "collision.ybn");
    assert!(json.get("scanned_file_count").is_none());
  }
}
