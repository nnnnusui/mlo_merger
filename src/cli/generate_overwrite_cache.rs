use std::path::PathBuf;

use bpaf::*;

use super::common::{self, CommonOptions};

/// Arguments for deriving what the overwrites call for.
#[derive(Debug, Clone)]
pub struct GenerateOverwriteCache {
  /// Source resource directory.
  pub source_dir: PathBuf,
  /// Overwrites the cache is derived from.
  pub overwrite_dir: PathBuf,
  /// Resource exclusions applied only to this execution.
  pub exclusions: Option<Vec<String>>,
  /// Shared execution options; the output is the overwrite cache.
  pub common: CommonOptions,
}

/// Parses `--generate-overwrite-cache` arguments.
pub fn parser() -> impl Parser<GenerateOverwriteCache> {
  let command = long("generate-overwrite-cache")
    .help("Build or update what the overwrites call for: the sources of added YMAPs and the overwrite warnings")
    .req_flag(());
  let source_dir = short('i')
    .long("input")
    .help("Source resource directory (default: asset/source)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/source"));
  let overwrite_dir = long("overwrite")
    .help("Overwrites the cache is derived from (default: asset/overwrite)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/overwrite"));
  let common = common::parser(PathBuf::from("asset/overwrite-cache"));
  let exclusions = super::generate_source_cache::exclusions_parser();
  construct!(command, source_dir, overwrite_dir, common, exclusions).map(
    |(_, source_dir, overwrite_dir, common, exclusions)| GenerateOverwriteCache {
      source_dir,
      overwrite_dir,
      exclusions,
      common,
    },
  )
}

/// Updates the vanilla and source caches, then builds or reuses the overwrite cache.
pub fn run(command: GenerateOverwriteCache) -> Result<String, Box<dyn std::error::Error>> {
  super::generate_vanilla_cache::update(&command.common)?;
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
  let written = crate::core::overwrite_cache::BuildOverwriteCache {
    source_dir: command.source_dir,
    source_cache_dir: command.common.source_cache,
    vanilla_cache_dir: command.common.vanilla_cache,
    overwrite_dir: Some(command.overwrite_dir),
    output_dir: command.common.output,
  }
  .run()?;
  Ok(
    if written {
      "Generated overwrite cache.\n"
    } else {
      "Overwrite cache is current; nothing to do.\n"
    }
    .to_string(),
  )
}
