//! Consistency of the source cache with its inventory.

use super::{Result, read_metadata, ymap_diff_path};
use crate::core::check::{CheckIssueKind, Findings};
use std::{collections::BTreeSet, path::Path};

/// Entries of the cache directory.
const CACHE_ENTRIES: [&str; 8] = [
  "cache_info.json",
  "stream_conflicts.json",
  "vanilla_ymaps_to_read.json",
  "source_cache.log",
  "stream_file_rtree.json",
  "resources",
  "diff",
  // Archived history, which nothing reads.
  "_old",
];

/// Checks that each file the inventory records as moved into the cache is there with its
/// recorded size (and SHA-256 when `deep`), that `resources/` holds nothing else, and that each
/// diff document belongs to a source YMAP replacing a vanilla one.
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
        if format == ".ymap" && file.vanilla.is_some() {
          let diff = Path::new("diff").join(ymap_diff_path(&resource.source, &file.path)?);
          diffs.insert(diff.to_string_lossy().replace('\\', "/"));
        }
      }
    }
  }
  findings.unrecorded_below(dir, "resources", &cached)?;
  findings.unrecorded_below(dir, "diff", &diffs)?;
  findings.unknown_entries(dir, &CACHE_ENTRIES)
}
