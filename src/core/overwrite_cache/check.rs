//! Checks the overwrite cache against what it was built from and the files it names.

use super::{
  ADDED_STREAM_FILES, AddedStreamFiles, FORMAT_VERSION, OverwriteWarnings, Result, SOURCE_UPDATES,
  SourceUpdate, WARNINGS, inputs_digest,
};
use crate::common::json::read_json;
use crate::core::check::{CheckIssueKind, Findings, files_below};
use crate::core::config::matching;
use std::collections::BTreeSet;
use std::path::Path;

/// Records the issues of the overwrite cache at `cache_dir`: a record that cannot be read, or
/// that was built from other source and vanilla caches or overwrites than those at
/// `source_cache_dir`, `vanilla_cache_dir` and `overwrite_dir`; added stream files whose
/// sources the source cache no longer holds; source updates whose earlier version its history
/// does not hold; and entries the cache does not write.
pub(crate) fn check(
  cache_dir: &Path,
  source_cache_dir: &Path,
  vanilla_cache_dir: &Path,
  overwrite_dir: Option<&Path>,
  findings: &mut Findings,
) -> Result<()> {
  let added = match read_json::<AddedStreamFiles>(cache_dir.join(ADDED_STREAM_FILES)) {
    Ok(added) => added,
    Err(error) => {
      findings.add(CheckIssueKind::Unreadable, ADDED_STREAM_FILES, Some(error.to_string()));
      return Ok(());
    }
  };
  let current =
    matching::with_config(|| inputs_digest(source_cache_dir, vanilla_cache_dir, overwrite_dir))?;
  match current {
    Ok(digest) if added.format_version == FORMAT_VERSION && added.inputs == digest => {}
    Ok(_) => findings.add(
      CheckIssueKind::Stale,
      ADDED_STREAM_FILES,
      Some("built from other caches or overwrites; run --generate-overwrite-cache".into()),
    ),
    Err(error) => findings.add(
      CheckIssueKind::Stale,
      ADDED_STREAM_FILES,
      Some(format!("cannot read what it was built from: {error}")),
    ),
  }
  if let Err(error) = read_json::<OverwriteWarnings>(cache_dir.join(WARNINGS)) {
    findings.add(CheckIssueKind::Unreadable, WARNINGS, Some(error.to_string()));
  }

  let cached = crate::core::source_cache::cached_files(source_cache_dir)
    .map(|files| files.into_iter().map(|file| file.cached_path.to_ascii_lowercase()).collect())
    .unwrap_or_else(|_| BTreeSet::new());
  for (name, file) in &added.files {
    for source in &file.sources {
      let cached_path = format!(
        "resources/{}/{}",
        crate::core::source_cache::resource_name(&source.resource),
        source.stream_path
      )
      .to_ascii_lowercase();
      if !cached.contains(&cached_path) {
        findings.add(
          CheckIssueKind::Missing,
          ADDED_STREAM_FILES,
          Some(format!(
            "{name} names {} of {}, which the source cache does not hold",
            source.stream_path, source.resource
          )),
        );
      }
    }
  }

  for record in files_below(cache_dir, SOURCE_UPDATES)? {
    let update = match read_json::<SourceUpdate>(cache_dir.join(&record)) {
      Ok(update) => update,
      Err(error) => {
        findings.add(CheckIssueKind::Unreadable, &record, Some(error.to_string()));
        continue;
      }
    };
    // History is never removed, so the earlier version stays; the current one may not.
    for path in [&update.before.source, &update.before.diff].into_iter().flatten() {
      if !crate::platform::fs::is_file(source_cache_dir.join(path)) {
        findings.add(
          CheckIssueKind::Missing,
          &record,
          Some(format!("names {path}, which the source cache does not hold")),
        );
      }
    }
  }
  findings.unknown_entries(cache_dir, &[ADDED_STREAM_FILES, WARNINGS, SOURCE_UPDATES])
}
