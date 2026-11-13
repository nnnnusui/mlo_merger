use std::path::{Path, PathBuf};

use bpaf::*;

use super::common::{self, CommonOptions};

/// Arguments for merging cached source conflicts into vanilla stream files.
#[derive(Debug, Clone)]
pub struct Merge {
  /// Source resource directory.
  pub source_dir: PathBuf,
  /// Resource exclusions applied only to this execution.
  pub exclusions: Option<Vec<String>>,
  /// Overwrites applied to source diffs before merging.
  pub overwrite_dir: PathBuf,
  /// What the overwrites call for, derived from them before merging.
  pub overwrite_cache_dir: PathBuf,
  /// Shared execution options.
  pub common: CommonOptions,
}

/// Parses `--merge` arguments.
pub fn parser(asset: &Path) -> impl Parser<Merge> + use<> {
  let command = long("merge").help("Merge source conflicts into vanilla stream files").req_flag(());
  let source_dir = short('i')
    .long("input")
    .help("Source resource directory (default: asset/source)")
    .argument::<PathBuf>("DIR")
    .fallback(asset.join("source"));
  let overwrite_dir = long("overwrite")
    .help("Overwrites applied to source diffs before merging (default: asset/overwrite)")
    .argument::<PathBuf>("DIR")
    .fallback(asset.join("overwrite"));
  let overwrite_cache_dir = long("overwrite-cache")
    .help("What the overwrites call for, derived from them (default: asset/overwrite-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(asset.join("overwrite-cache"));
  let common = common::parser(asset, asset.join("merged"));
  let exclusions = super::generate_source_cache::exclusions_parser();
  construct!(command, source_dir, overwrite_dir, overwrite_cache_dir, common, exclusions).map(
    |(_, source_dir, overwrite_dir, overwrite_cache_dir, common, exclusions)| Merge {
      source_dir,
      common,
      exclusions,
      overwrite_dir,
      overwrite_cache_dir,
    },
  )
}

/// Updates the vanilla, source and overwrite caches, then merges supported source changes into
/// the latest vanilla stream files.
///
/// `--force` applies only to the merge; the caches are rebuilt only when their inputs changed.
pub fn run(command: Merge) -> Result<String, Box<dyn std::error::Error>> {
  super::generate_overwrite_cache::update(
    &command.common,
    &command.source_dir,
    command.exclusions,
    &command.overwrite_dir,
    &command.overwrite_cache_dir,
  )?;
  crate::core::merge::Merge {
    source_dir: command.source_dir,
    vanilla_cache_dir: command.common.vanilla_cache,
    source_cache_dir: command.common.source_cache,
    output_dir: command.common.output,
    overwrite_dir: Some(command.overwrite_dir),
    overwrite_cache_dir: command.overwrite_cache_dir,
    force: command.common.force,
  }
  .run()?;
  Ok(String::new())
}
