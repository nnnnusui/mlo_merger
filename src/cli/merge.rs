use std::path::PathBuf;

use bpaf::*;

use super::common::{self, CommonOptions};

/// Arguments for merging cached source conflicts into vanilla stream files.
#[derive(Debug, Clone)]
pub struct Merge {
  /// Source resource directory.
  pub source_dir: PathBuf,
  /// Resource-local light edit directory.
  pub overwrite_dir: PathBuf,
  /// Resource exclusions applied only to this execution.
  pub exclusions: Option<Vec<String>>,
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
  let exclusions = super::generate_source_cache::exclusions_parser();
  construct!(command, source_dir, common, overwrite_dir, exclusions).map(
    |(_, source_dir, common, overwrite_dir, exclusions)| Merge {
      source_dir,
      common,
      overwrite_dir,
      exclusions,
    },
  )
}

/// Merges supported source changes into the latest vanilla stream files.
pub fn run(command: Merge) -> Result<(), Box<dyn std::error::Error>> {
  crate::core::merge::Merge {
    source_dir: command.source_dir,
    vanilla_dir: command.common.vanilla,
    vanilla_cache_dir: command.common.vanilla_cache,
    source_cache_dir: command.common.source_cache,
    output_dir: command.common.output,
    overwrite_dir: command.overwrite_dir,
    exclusions: command.exclusions,
    force: command.common.force,
  }
  .run()
}
