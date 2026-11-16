//! Read-only consistency checks across map artifact stores.

#[cfg(test)]
mod tests;

use crate::core::check::{CheckIssue, CheckReport, Findings};
use crate::platform::fs;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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
  /// let report = mlo_merger::app::map::Check {
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
      crate::core::vanilla_cache::check(&self.vanilla_cache_dir, deep, findings)
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
