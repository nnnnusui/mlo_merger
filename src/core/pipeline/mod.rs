//! Runs every cached stage, from vanilla extraction to deployment.

mod report;
#[cfg(test)]
mod tests;

use super::{
  deploy::Deploy, merge::Merge, source_cache::BuildSourceCache, vanilla::GenerateVanilla,
  vanilla_cache::BuildVanillaCache,
};
use crate::common::path::absolute_path;
use crate::core::config::gamebuild::GameBuild;
use crate::platform::fs;
use crate::platform::process;
use std::path::PathBuf;

pub use report::{PipelineChanges, PipelineReport, PipelineSummary};

/// Executes the cached vanilla, source, merge and deployment stages in order.
#[derive(Debug, Clone)]
pub struct Pipeline {
  /// Source resources, passed to source-cache generation and merge.
  pub source_dir: PathBuf,
  /// Installed game directory used only to create missing vanilla data.
  pub game_dir: PathBuf,
  /// CodeWalker's `magic.dat`, used only to create missing vanilla data.
  pub magic: PathBuf,
  /// Raw vanilla archive directory.
  pub vanilla_dir: PathBuf,
  /// Derived vanilla artifact directory.
  pub vanilla_cache_dir: PathBuf,
  /// Derived source artifact directory.
  pub source_cache_dir: PathBuf,
  /// Resource exclusions for this run; absent or empty means no exclusions.
  pub exclusions: Option<Vec<String>>,
  /// Intermediate merged artifact directory.
  pub merged_dir: PathBuf,
  /// Overwrites applied to source diffs before merging; `None` applies none.
  pub overwrite_dir: Option<PathBuf>,
  /// What the overwrites call for, built from them before merging.
  pub overwrite_cache_dir: PathBuf,
  /// Final deployment directory.
  pub output_dir: PathBuf,
  /// Game build whose last stage the vanilla cache reflects; `None` is the last stage.
  pub gamebuild: Option<GameBuild>,
  /// Forces the derived stages without re-extracting an existing raw archive.
  pub force: bool,
}

impl Pipeline {
  /// Runs necessary extraction, derived caches, incremental merge and deployment.
  ///
  /// The executable initializes the shared logger before calling this method.
  ///
  /// ```no_run
  /// use mlo_merger::core::pipeline::Pipeline;
  /// Pipeline {
  ///   source_dir: "asset/source".into(), game_dir: "/mnt/gtav".into(), magic: "magic.dat".into(),
  ///   vanilla_dir: "asset/vanilla".into(), vanilla_cache_dir: "asset/vanilla-cache".into(),
  ///   source_cache_dir: "asset/source-cache".into(), merged_dir: "asset/merged".into(),
  ///   exclusions: None, overwrite_dir: Some("asset/overwrite".into()),
  ///   overwrite_cache_dir: "asset/overwrite-cache".into(),
  ///   output_dir: "merged_mlo".into(), gamebuild: None, force: false,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<PipelineSummary, Box<dyn std::error::Error>> {
    crate::core::config::matching::with_config(|| self.run_steps())?
  }

  fn run_steps(&self) -> Result<PipelineSummary, Box<dyn std::error::Error>> {
    if !fs::is_dir(&self.source_dir) {
      return Err(
        format!("Source resource directory does not exist: {}", self.source_dir.display()).into(),
      );
    }
    self.validate_paths()?;

    tracing::info!("Pipeline: generate-vanilla");
    GenerateVanilla {
      game_dir: self.game_dir.clone(),
      output_dir: self.vanilla_dir.clone(),
      magic: self.magic.clone(),
      reuse_existing: true,
    }
    .run()?;

    tracing::info!("Pipeline: generate-vanilla-cache");
    let vanilla_cache = BuildVanillaCache {
      vanilla_dir: self.vanilla_dir.clone(),
      output_dir: self.vanilla_cache_dir.clone(),
      gamebuild: self.gamebuild.clone(),
      force: self.force,
    }
    .run()?;
    tracing::info!("{}", vanilla_cache.summary());

    tracing::info!("Pipeline: generate-source-cache");
    let source_cache = BuildSourceCache {
      source_dir: self.source_dir.clone(),
      output_dir: self.source_cache_dir.clone(),
      vanilla_dir: self.vanilla_dir.clone(),
      vanilla_cache_dir: self.vanilla_cache_dir.clone(),
      resource: None,
      exclusions: self.exclusions.clone(),
      force: self.force,
    }
    .run()?;

    tracing::info!("Pipeline: generate-overwrite-cache");
    crate::core::overwrite_cache::BuildOverwriteCache {
      source_dir: self.source_dir.clone(),
      source_cache_dir: self.source_cache_dir.clone(),
      vanilla_cache_dir: self.vanilla_cache_dir.clone(),
      overwrite_dir: self.overwrite_dir.clone(),
      output_dir: self.overwrite_cache_dir.clone(),
    }
    .run()?;

    tracing::info!("Pipeline: merge");
    let merged = Merge {
      source_dir: self.source_dir.clone(),
      vanilla_cache_dir: self.vanilla_cache_dir.clone(),
      source_cache_dir: self.source_cache_dir.clone(),
      output_dir: self.merged_dir.clone(),
      overwrite_dir: self.overwrite_dir.clone(),
      overwrite_cache_dir: self.overwrite_cache_dir.clone(),
      force: self.force,
    }
    .run()?;

    tracing::info!("Pipeline: deploy");
    let deploy = Deploy {
      merged_dir: self.merged_dir.clone(),
      source_cache_dir: self.source_cache_dir.clone(),
      output_dir: self.output_dir.clone(),
      force: self.force,
    }
    .run()?;
    Ok(PipelineSummary {
      changes: PipelineChanges {
        vanilla_cache: vanilla_cache.changes.under(&self.vanilla_dir),
        source_cache: source_cache.changes.under(&self.source_cache_dir),
        merged: merged.changes.under(&self.merged_dir),
      },
      warnings: merged.warnings,
      deploy,
    })
  }

  /// Source nesting is intentional: source can be `resources` while execution and
  /// generated artifacts live under `resources/mlo_merged`. Only artifact
  /// paths are checked for overlap; the source root is excluded from this list.
  fn validate_paths(&self) -> Result<(), Box<dyn std::error::Error>> {
    let paths = [
      ("vanilla", &self.vanilla_dir),
      ("vanilla-cache", &self.vanilla_cache_dir),
      ("source-cache", &self.source_cache_dir),
      ("merged", &self.merged_dir),
      ("deployment", &self.output_dir),
    ]
    .into_iter()
    .chain([("overwrite-cache", &self.overwrite_cache_dir)])
    .map(|(name, path)| Ok((name, absolute_path(path)?)))
    .collect::<Result<Vec<_>, std::io::Error>>()?;
    let workspace = fs::canonicalize(&process::current_dir()?)?;
    for (index, (name, path)) in paths.iter().enumerate() {
      if *name != "vanilla" && workspace.starts_with(path) {
        return Err(format!("Pipeline {name} output overlaps the workspace").into());
      }
      for (other_name, other) in paths.iter().skip(index + 1) {
        if path.starts_with(other) || other.starts_with(path) {
          return Err(format!("Pipeline paths overlap: {name} and {other_name}").into());
        }
      }
    }
    Ok(())
  }
}
