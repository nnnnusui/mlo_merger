use std::path::PathBuf;

use bpaf::*;

use super::common::{self, CommonOptions};

/// Arguments for merging cached source conflicts into vanilla stream files.
#[derive(Debug, Clone)]
pub struct Merge {
  /// Source resource directory.
  pub source_dir: PathBuf,
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
  let exclusions = super::generate_source_cache::exclusions_parser();
  construct!(command, source_dir, common, exclusions).map(|(_, source_dir, common, exclusions)| {
    Merge {
      source_dir,
      common,
      exclusions,
    }
  })
}

/// Updates the vanilla and source caches, then merges supported source changes into the latest
/// vanilla stream files.
///
/// `--force` applies only to the merge; the caches are rebuilt only when their inputs changed.
pub fn run(command: Merge) -> Result<(), Box<dyn std::error::Error>> {
  super::generate_vanilla_cache::update(&command.common.vanilla, &command.common.vanilla_cache)?;
  crate::core::source_cache::BuildSourceCache {
    source_dir: command.source_dir.clone(),
    output_dir: command.common.source_cache.clone(),
    vanilla_dir: command.common.vanilla,
    vanilla_cache_dir: command.common.vanilla_cache.clone(),
    resource: None,
    exclusions: command.exclusions,
    force: false,
  }
  .run()?;
  crate::core::merge::Merge {
    source_dir: command.source_dir,
    vanilla_cache_dir: command.common.vanilla_cache,
    source_cache_dir: command.common.source_cache,
    output_dir: command.common.output,
    force: command.common.force,
  }
  .run()
}
