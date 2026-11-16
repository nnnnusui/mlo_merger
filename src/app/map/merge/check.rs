//! Consistency of the merge output with its metadata.

use super::{incremental::MergeMetadata, operation::Result, outputs::CACHE_INFO};
use crate::core::check::{CheckIssueKind, Findings};
use std::{collections::BTreeSet, path::Path};

/// Entries of the merge output besides its metadata.
const OUTPUT_ENTRIES: [&str; 6] =
  [CACHE_INFO, "duplicates.json", "_omit.txt", "stream_file_rtree.json", "ymap", "ybn"];

/// Checks that each merged file the metadata records is there with its recorded size (and
/// SHA-256 when `deep`), and that `ymap/` and `ybn/` hold nothing else.
pub(crate) fn check(
  dir: &Path,
  deep: bool,
  findings: &mut Findings,
) -> Result<()> {
  let metadata = match MergeMetadata::load(dir) {
    Ok(Some(metadata)) => metadata,
    Ok(None) => {
      findings.add(
        CheckIssueKind::Unreadable,
        CACHE_INFO,
        Some("missing or of another schema".into()),
      );
      return Ok(());
    }
    Err(error) => {
      findings.add(CheckIssueKind::Unreadable, CACHE_INFO, Some(error.to_string()));
      return Ok(());
    }
  };
  let mut recorded = BTreeSet::new();
  for output in metadata.files.values().filter_map(|record| record.output.as_ref()) {
    let path = output.path.replace('\\', "/");
    findings.recorded_file(
      dir,
      &path,
      Some(output.fingerprint.size),
      &output.fingerprint.sha256,
      deep,
    )?;
    recorded.insert(path);
  }
  findings.unrecorded_below(dir, "ymap", &recorded)?;
  findings.unrecorded_below(dir, "ybn", &recorded)?;
  findings.unknown_entries(dir, &OUTPUT_ENTRIES)
}
