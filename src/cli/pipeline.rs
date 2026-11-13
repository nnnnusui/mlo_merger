use super::common::{self, CommonOptions};
use bpaf::*;
use std::path::{Path, PathBuf};

/// Arguments for the `--pipeline` vanilla-to-deployment run.
#[derive(Debug, Clone)]
pub struct Pipeline {
  /// Resource input passed to source-cache generation and merge.
  pub source_dir: PathBuf,
  /// Installed game used only when vanilla generation is necessary.
  pub game_dir: PathBuf,
  /// CodeWalker's `magic.dat`, used only when vanilla generation is necessary.
  pub magic: PathBuf,
  /// Intermediate merged artifact directory.
  pub merged_dir: PathBuf,
  /// Source resource exclusions for this execution only.
  pub exclusions: Option<Vec<String>>,
  /// Overwrites applied to source diffs before merging.
  pub overwrite_dir: PathBuf,
  /// What the overwrites call for, derived from them before merging.
  pub overwrite_cache_dir: PathBuf,
  /// Shared artifact paths and deployment output options.
  pub common: CommonOptions,
}

/// Parses `--pipeline` arguments.
pub fn parser(asset: &Path) -> impl Parser<Pipeline> + use<> {
  options(asset)
    .to_options()
    .descr("Run the cached pipeline: vanilla, caches, merge and deployment")
    .command("--pipeline")
}

fn options(asset: &Path) -> impl Parser<Pipeline> + use<> {
  let source_dir = short('i')
    .long("input")
    .help("Source resource directory (default: asset/source)")
    .argument::<PathBuf>("DIR")
    .fallback(asset.join("source"));
  let game_dir = long("game-dir")
    .help("Game directory for missing vanilla archives (default: /mnt/gtav)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("/mnt/gtav"));
  let magic = super::generate_vanilla::magic_parser();
  let merged_dir = long("merged")
    .help("Intermediate merged directory (default: asset/merged)")
    .argument::<PathBuf>("DIR")
    .fallback(asset.join("merged"));
  let overwrite_dir = long("overwrite")
    .help("Overwrites applied to source diffs before merging (default: asset/overwrite)")
    .argument::<PathBuf>("DIR")
    .fallback(asset.join("overwrite"));
  let overwrite_cache_dir = long("overwrite-cache")
    .help("What the overwrites call for, derived from them (default: asset/overwrite-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(asset.join("overwrite-cache"));
  let common = common::parser(asset, asset.join("merged_mlo"));
  let exclusions = super::generate_source_cache::exclusions_parser();
  construct!(
    source_dir,
    game_dir,
    magic,
    merged_dir,
    overwrite_dir,
    overwrite_cache_dir,
    common,
    exclusions
  )
  .map(
    |(
      source_dir,
      game_dir,
      magic,
      merged_dir,
      overwrite_dir,
      overwrite_cache_dir,
      common,
      exclusions,
    )| Pipeline {
      source_dir,
      game_dir,
      magic,
      merged_dir,
      common,
      exclusions,
      overwrite_dir,
      overwrite_cache_dir,
    },
  )
}

/// Runs all cached stages with deployment-scoped `-o`; prints the report as one line of JSON.
pub fn run(command: Pipeline) -> Result<String, Box<dyn std::error::Error>> {
  if !command.common.step_names.is_empty() {
    return Err(
      "--step-name selection is not implemented for --pipeline; use an explicit operation".into(),
    );
  }
  let pipeline = crate::core::pipeline::Pipeline {
    source_dir: command.source_dir,
    game_dir: command.game_dir,
    magic: command.magic,
    vanilla_dir: command.common.vanilla,
    vanilla_cache_dir: command.common.vanilla_cache,
    source_cache_dir: command.common.source_cache,
    exclusions: command.exclusions,
    merged_dir: command.merged_dir,
    overwrite_dir: Some(command.overwrite_dir),
    overwrite_cache_dir: command.overwrite_cache_dir,
    output_dir: command.common.output,
    gamebuild: super::common::gamebuild(command.common.gamebuild.as_deref())?,
    force: command.common.force,
  };
  let result = pipeline.run()?;
  tracing::info!(
    "Pipeline complete: {} files deployed; {} unchanged; {} removed.",
    result.deploy.copied,
    result.deploy.skipped,
    result.deploy.removed
  );
  let report = crate::core::pipeline::PipelineReport {
    changes: result.changes,
    warnings: result.warnings,
  };
  Ok(format!("{}\n", serde_json::to_string(&report)?))
}
