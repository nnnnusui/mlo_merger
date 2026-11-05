//! Consistency of the raw vanilla archive with its manifest.

use super::{Result, manifest::load_manifest};
use crate::core::check::{CheckIssueKind, Findings, files_below};
use std::{collections::BTreeSet, path::Path};

/// Files of the archive besides the stages' objects.
const ARCHIVE_FILES: [&str; 3] = ["cache_info.json", "hash_names.json", "rpf_names.json"];

/// Checks that every object the manifest records exists, with its SHA-256 when `deep`, and
/// that the archive holds nothing else.
pub(crate) fn check(
  dir: &Path,
  deep: bool,
  findings: &mut Findings,
) -> Result<()> {
  let manifest = match load_manifest(dir) {
    Ok(manifest) => manifest,
    Err(error) => {
      findings.add(CheckIssueKind::Unreadable, "cache_info.json", Some(error.to_string()));
      return Ok(());
    }
  };
  let mut recorded = ARCHIVE_FILES.map(String::from).into_iter().collect::<BTreeSet<_>>();
  for version in &manifest.versions {
    recorded.insert(format!("{}/version_info.json", version.id));
    for change in version.changes.values() {
      let object = change.file.object.replace('\\', "/");
      findings.recorded_file(dir, &object, None, &change.file.sha256, deep)?;
      recorded.insert(object);
    }
  }
  for path in files_below(dir, "")? {
    if !recorded.contains(&path) {
      findings.add(CheckIssueKind::Unrecorded, &path, None);
    }
  }
  Ok(())
}
