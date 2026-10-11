use std::path::PathBuf;

use bpaf::*;
use simplelog::{ColorChoice, CombinedLogger, Config, LevelFilter, TermLogger, TerminalMode};

use super::common::{self, CommonOptions};

/// Arguments for merging cached source conflicts into vanilla stream files.
#[derive(Debug, Clone)]
pub struct Merge {
  /// Source resource directory.
  pub source_dir: PathBuf,
  /// Resource-local light edit directory.
  pub overwrite_dir: PathBuf,
  /// Shared execution options.
  pub common: CommonOptions,
}

/// Parses `--merge` arguments.
pub fn parser() -> impl Parser<Merge> {
  let command = long("merge").help("Merge source conflicts into vanilla stream files").req_flag(());
  let source_dir = short('i')
    .long("input")
    .help("Source resource directory (default: asset/source)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/source"));
  let common = common::parser(PathBuf::from("asset/merged"));
  let overwrite_dir =
    long("overwrite-dir").argument::<PathBuf>("DIR").fallback("asset/overwrite".into());
  construct!(command, source_dir, common, overwrite_dir).map(
    |(_, source_dir, common, overwrite_dir)| Merge {
      source_dir,
      common,
      overwrite_dir,
    },
  )
}

/// Merges supported source changes into the latest vanilla stream files.
pub fn run(command: Merge) -> Result<(), Box<dyn std::error::Error>> {
  CombinedLogger::init(vec![TermLogger::new(
    LevelFilter::Info,
    Config::default(),
    TerminalMode::Mixed,
    ColorChoice::Auto,
  )])?;
  crate::core::merge::merge::run_with_overwrite(
    &command.source_dir,
    &command.common.vanilla,
    &command.common.vanilla_cache,
    &command.common.source_cache,
    &command.common.output,
    command.common.force,
    &command.overwrite_dir,
  )
}
