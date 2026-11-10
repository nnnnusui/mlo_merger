//! Consistency of the source cache with its inventory.

use super::{
  Result,
  generate::propagate::{PROPAGATED_DIR, read_record},
  read_metadata, ymap_diff_path,
};
use crate::core::check::{CheckIssueKind, Findings};
use crate::platform::fs;
use std::{collections::BTreeSet, path::Path};

/// Entries of the cache directory.
const CACHE_ENTRIES: [&str; 11] = [
  "cache_info.json",
  "stream_conflicts.json",
  "vanilla_ymaps_to_read.json",
  "source_cache.log",
  "stream_file_rtree.json",
  "resources",
  "diff",
  "propagated",
  "added_ymap_relationships.json",
  // Archived resources, which nothing reads.
  "_old",
  // Earlier versions of cached files, which overwrites written against them read.
  "history",
];

/// Checks that each file the inventory records as moved into the cache is there with its
/// recorded size (and SHA-256 when `deep`), that `resources/` holds nothing else, and that each
/// diff document belongs to a source YMAP replacing a vanilla one or one a resource adds.
pub(crate) fn check(
  dir: &Path,
  deep: bool,
  findings: &mut Findings,
) -> Result<()> {
  let metadata = match read_metadata(dir) {
    Ok(metadata) => metadata,
    Err(error) => {
      findings.add(CheckIssueKind::Unreadable, "cache_info.json", Some(error.to_string()));
      return Ok(());
    }
  };
  let mut cached = BTreeSet::new();
  let mut diffs = BTreeSet::from(["diff/inputs.json".to_string()]);
  for resource in metadata.resources.values() {
    for (format, files) in &resource.files_by_format {
      for file in files {
        if let Some(path) = file.cached_path.as_ref().filter(|path| !path.starts_with("_old/")) {
          findings.recorded_file(dir, path, Some(file.size), &file.sha256, deep)?;
          cached.insert(path.clone());
        }
        let added = super::added::is_added_stream_file(format, file) && file.cached_path.is_some();
        if format == ".ymap" && (file.vanilla.is_some() || added) {
          let diff = Path::new(super::DIFF_DIR).join(ymap_diff_path(&resource.source, &file.path)?);
          diffs.insert(diff.to_string_lossy().replace('\\', "/"));
        }
      }
    }
  }
  findings.unrecorded_below(dir, "resources", &cached)?;
  findings.unrecorded_below(dir, "diff", &diffs)?;
  if fs::is_dir(dir.join(PROPAGATED_DIR)) {
    let mut propagated = BTreeSet::from([format!("{PROPAGATED_DIR}/inputs.json")]);
    match read_record(dir) {
      Some(record) => {
        for document in record.documents {
          if !fs::is_file(dir.join(&document.path)) {
            findings.add(CheckIssueKind::Missing, &document.path, None);
          }
          propagated.insert(document.path);
        }
      }
      None => {
        findings.add(CheckIssueKind::Unreadable, format!("{PROPAGATED_DIR}/inputs.json"), None)
      }
    }
    findings.unrecorded_below(dir, PROPAGATED_DIR, &propagated)?;
  }
  findings.unknown_entries(dir, &CACHE_ENTRIES)
}
