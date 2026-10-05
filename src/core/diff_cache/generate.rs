//! Coordinates diff-cache generation across manifest-discovered resources.

use super::{
  ModelReader, Result,
  io::Scratch,
  metadata::{history, load_manifest},
  resource::generate_resource,
  types::GenerationReport,
  vanilla::NativeVariants,
};
use crate::core::{
  common::function::get_resource_directories,
  gtav_cache::{VersionLog, read_ymap, write_json},
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

/// Generates vanilla-relative differences for one or more FiveM resources.
#[derive(Debug, Clone)]
pub struct BuildDiffCache {
  /// Single resource or resources root, including bracket-group layouts.
  pub input_dir: PathBuf,
  /// Empty destination for generation metadata, logs and per-resource reports.
  pub output_dir: PathBuf,
  /// GTAV cache root; the CLI defaults to asset/gtav-cache when omitted.
  pub gtav_cache_dir: PathBuf,
}

impl BuildDiffCache {
  /// Infers the latest of each resource's per-file best stages and saves differences and metadata.
  ///
  /// ```no_run
  /// simplelog::CombinedLogger::init(vec![mlo_merger::core::gtav_cache::version_logger()])?;
  /// let command = mlo_merger::core::diff_cache::BuildDiffCache {
  ///   input_dir: "asset/source".into(), output_dir: "asset/diff-cache".into(),
  ///   gtav_cache_dir: "asset/gtav-cache".into(),
  /// };
  /// command.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<()> {
    self.run_with_reader(read_ymap)
  }

  pub(super) fn run_with_reader(
    &self,
    reader: ModelReader,
  ) -> Result<()> {
    let input = self.input_dir.canonicalize()?;
    let resources = get_resource_directories(&input)?;
    if resources.is_empty() {
      return Err(format!("No FiveM resources found in {}", input.display()).into());
    }
    let cache = self.gtav_cache_dir.canonicalize()?;
    let manifest = load_manifest(&cache)?;
    let histories = history(&manifest)?;
    if self.output_dir.exists() && fs::read_dir(&self.output_dir)?.next().is_some() {
      return Err(format!("Output directory is not empty: {}", self.output_dir.display()).into());
    }
    fs::create_dir_all(&self.output_dir)?;
    let log = VersionLog::start(&self.output_dir)?;
    let generated_at = chrono::Utc::now().to_rfc3339();
    let mut report = GenerationReport {
      format_version: 1,
      input: input.clone(),
      gtav_cache: cache.clone(),
      generated_at: generated_at.clone(),
      completed: false,
      error: None,
      distance_metric: "model_field_changes_v1; ignore name/block; latest changed stage wins ties",
      resources: Vec::new(),
    };
    let result = (|| -> Result<()> {
      let scratch_path = self.output_dir.join(".working");
      fs::create_dir(&scratch_path)?;
      let scratch = Scratch(scratch_path);
      let mut provider = NativeVariants {
        cache: &cache,
        game_dir: &manifest.game_dir,
        scratch: &scratch.0,
        reader,
        codewalker: None,
        recovered: BTreeMap::new(),
        models: BTreeMap::new(),
        extractions: 0,
        replaying: BTreeSet::new(),
      };
      for resource in resources {
        let relative = resource.strip_prefix(&input)?;
        let id = if relative.as_os_str().is_empty() {
          resource
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Resource name is not UTF-8")?
            .to_string()
        } else {
          relative.to_string_lossy().replace('\\', "/")
        };
        report.resources.push(generate_resource(
          &resource,
          &id,
          &generated_at,
          &self.output_dir,
          &manifest,
          &histories,
          &mut provider,
        )?);
      }
      Ok(())
    })();
    if let Err(error) = &result {
      log::error!("Diff cache generation failed: {error}");
      report.error = Some(error.to_string());
    } else {
      report.completed = true;
      log::info!(
        "Generated diff cache for {} resources at {}",
        report.resources.len(),
        self.output_dir.display()
      );
    }
    write_json(&self.output_dir.join("diff_cache_info.json"), &report)?;
    log.finish()?;
    result
  }
}
