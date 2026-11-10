//! Read-only consistency checks of every store against its own metadata.
//!
//! Each store's module checks its own files, as it owns their layout: recorded files that are
//! missing or changed, files its metadata does not record, and, for the vanilla cache, links
//! that are broken or point elsewhere. Nothing is repaired; the next run of the store's stage
//! does that, except for unrecorded files, which no stage removes.

mod findings;
#[cfg(test)]
mod tests;

use crate::platform::fs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub(crate) use findings::{Findings, files_below};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// What [`Check::run`] found.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct CheckReport {
  pub issues: Vec<CheckIssue>,
}

/// One inconsistency between a store and its metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct CheckIssue {
  /// `vanilla`, `vanilla-cache`, `source-cache`, `overwrite-cache`, `merged` or `deployment`.
  pub store: String,
  pub kind: CheckIssueKind,
  /// The file, relative to the store's directory.
  pub path: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum CheckIssueKind {
  /// The metadata records the file, but it does not exist.
  Missing,
  /// The file exists, but the metadata does not record it.
  Unrecorded,
  /// The file's size, or with `--deep` its SHA-256, differs from the record.
  Changed,
  /// A link whose target does not exist.
  BrokenLink,
  /// A link, or a file in its place, that does not point where the metadata says.
  WrongLink,
  /// The metadata cannot be read, so the store was not checked further.
  Unreadable,
  /// The store was built from inputs that changed since; its stage rebuilds it.
  Stale,
}

/// Checks the stores at these directories; a store whose directory is missing is skipped.
#[derive(Debug, Clone)]
pub struct Check {
  pub vanilla_dir: PathBuf,
  pub vanilla_cache_dir: PathBuf,
  pub source_cache_dir: PathBuf,
  /// The overwrites the overwrite cache is built from, if any.
  pub overwrite_dir: Option<PathBuf>,
  pub overwrite_cache_dir: PathBuf,
  pub merged_dir: PathBuf,
  /// Deployment output.
  pub output_dir: PathBuf,
  /// Hashes recorded files to compare their contents, not only their sizes.
  pub deep: bool,
}

impl Check {
  /// Checks every store without changing any file.
  ///
  /// ```no_run
  /// let report = mlo_merger::core::check::Check {
  ///   vanilla_dir: "asset/vanilla".into(), vanilla_cache_dir: "asset/vanilla-cache".into(),
  ///   source_cache_dir: "asset/source-cache".into(), overwrite_dir: Some("asset/overwrite".into()),
  ///   overwrite_cache_dir: "asset/overwrite-cache".into(), merged_dir: "asset/merged".into(),
  ///   output_dir: "asset/merged_mlo".into(), deep: false,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<CheckReport> {
    let deep = self.deep;
    let mut issues = Vec::new();
    check_store(&mut issues, "vanilla", &self.vanilla_dir, |findings| {
      crate::core::vanilla::check(&self.vanilla_dir, deep, findings)
    })?;
    check_store(&mut issues, "vanilla-cache", &self.vanilla_cache_dir, |findings| {
      crate::core::vanilla_cache::check(&self.vanilla_cache_dir, &self.vanilla_dir, deep, findings)
    })?;
    check_store(&mut issues, "source-cache", &self.source_cache_dir, |findings| {
      crate::core::source_cache::check(&self.source_cache_dir, deep, findings)
    })?;
    check_store(&mut issues, "overwrite-cache", &self.overwrite_cache_dir, |findings| {
      crate::core::overwrite_cache::check(
        &self.overwrite_cache_dir,
        &self.source_cache_dir,
        &self.vanilla_cache_dir,
        self.overwrite_dir.as_deref(),
        findings,
      )
    })?;
    check_store(&mut issues, "merged", &self.merged_dir, |findings| {
      crate::core::merge::check(&self.merged_dir, deep, findings)
    })?;
    check_store(&mut issues, "deployment", &self.output_dir, |findings| {
      crate::core::deploy::check(&self.output_dir, deep, findings)
    })?;
    for issue in &issues {
      let detail = issue.detail.as_deref().map(|detail| format!(": {detail}")).unwrap_or_default();
      tracing::warn!("{} {:?} {}{detail}", issue.store, issue.kind, issue.path);
    }
    Ok(CheckReport {
      issues,
    })
  }
}

/// Runs `check` on the store at `directory`, unless it does not exist, adding its issues.
fn check_store(
  issues: &mut Vec<CheckIssue>,
  store: &'static str,
  directory: &Path,
  check: impl FnOnce(&mut Findings) -> Result<()>,
) -> Result<()> {
  if !fs::is_dir(directory) {
    tracing::info!("Skipping the {store} check: {} does not exist", directory.display());
    return Ok(());
  }
  let mut findings = Findings::new(store);
  check(&mut findings)?;
  let found = findings.into_issues();
  tracing::info!("Checked {store}: {} issues", found.len());
  issues.extend(found);
  Ok(())
}
