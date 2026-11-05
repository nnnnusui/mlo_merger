//! Consistency of the deployment with its metadata.

use super::{Result, metadata::read_metadata};
use crate::core::check::{CheckIssueKind, Findings, files_below};
use std::path::Path;

/// Checks that each file the deployment metadata records is there as it was copied (its size,
/// and SHA-256 when `deep`), and that the managed `stream/{extension}/merged/` and
/// `stream/{extension}/clone/` directories hold nothing else. Other files are not deployment's.
pub(crate) fn check(
  output: &Path,
  deep: bool,
  findings: &mut Findings,
) -> Result<()> {
  let metadata = match read_metadata(&output.join("deploy_cache_info.json")) {
    Ok(metadata) => metadata,
    Err(error) => {
      findings.add(CheckIssueKind::Unreadable, "deploy_cache_info.json", Some(error.to_string()));
      return Ok(());
    }
  };
  for (relative, file) in &metadata.files {
    findings.recorded_file(output, relative, Some(file.output.size), &file.output.sha256, deep)?;
  }
  for path in files_below(output, "stream")? {
    let managed = path.split('/').nth(2).is_some_and(|area| matches!(area, "merged" | "clone"));
    if managed && !metadata.files.contains_key(&path) {
      findings.add(CheckIssueKind::Unrecorded, &path, None);
    }
  }
  Ok(())
}
