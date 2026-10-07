use super::common::{self, CommonOptions};
use bpaf::*;
use simplelog::{ColorChoice, CombinedLogger, Config, LevelFilter, TermLogger, TerminalMode};
use std::path::PathBuf;

/// Arguments for the default vanilla-to-deployment pipeline.
#[derive(Debug, Clone)]
pub struct Pipeline {
  /// Resource input passed to source-cache generation and merge.
  pub source_dir: PathBuf,
  /// Installed game used only when vanilla generation is necessary.
  pub game_dir: PathBuf,
  /// Intermediate merged artifact directory.
  pub merged_dir: PathBuf,
  /// Shared artifact paths and deployment output options.
  pub common: CommonOptions,
}

/// Parses the default operation without requiring a command flag.
pub fn parser() -> impl Parser<Pipeline> {
  let source_dir = short('i')
    .long("input")
    .help("Source resource directory (default: asset/source)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/source"));
  let game_dir = long("game-dir")
    .help("Game directory for missing vanilla archives (default: /mnt/gtav)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("/mnt/gtav"));
  let merged_dir = long("merged")
    .help("Intermediate merged directory (default: asset/merged)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/merged"));
  let common = common::parser(PathBuf::from("asset/merged_mlo"));
  construct!(source_dir, game_dir, merged_dir, common).map(
    |(source_dir, game_dir, merged_dir, common)| Pipeline {
      source_dir,
      game_dir,
      merged_dir,
      common,
    },
  )
}

/// Runs all cached stages with one logger and deployment-scoped `-o`.
pub fn run(command: Pipeline) -> Result<(), Box<dyn std::error::Error>> {
  if !command.common.step_names.is_empty() {
    return Err("--step-name selection is not implemented for the default pipeline; use an explicit operation".into());
  }
  if command.common.gamebuild.is_some() {
    return Err("--gamebuild ceilings are not implemented across the default pipeline; generate a limited vanilla archive explicitly".into());
  }
  CombinedLogger::init(vec![
    TermLogger::new(LevelFilter::Info, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
    crate::core::vanilla::version_logger(),
  ])?;
  let result = crate::core::pipeline::Pipeline {
    source_dir: command.source_dir,
    game_dir: command.game_dir,
    vanilla_dir: command.common.vanilla,
    vanilla_cache_dir: command.common.vanilla_cache,
    source_cache_dir: command.common.source_cache,
    merged_dir: command.merged_dir,
    output_dir: command.common.output,
    force: command.common.force,
  }
  .run()?;
  println!(
    "Pipeline complete: {} files deployed; {} unchanged; {} removed.",
    result.copied, result.skipped, result.removed
  );
  Ok(())
}
