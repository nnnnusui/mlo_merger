use std::path::PathBuf;

use bpaf::*;

use super::common::{self, CommonOptions};

/// Arguments for indexing source resources and their vanilla conflicts.
#[derive(Debug, Clone)]
pub struct GenerateSourceCache {
  /// Source resource directory.
  pub source_dir: PathBuf,
  /// Restricts updates to one resource name or relative resource path.
  pub resource: Option<String>,
  /// Resource exclusions for this execution only.
  pub exclusions: Option<Vec<String>>,
  /// Shared execution options.
  pub common: CommonOptions,
}

/// Parses `--generate-source-cache` arguments.
pub fn parser() -> impl Parser<GenerateSourceCache> {
  let command = long("generate-source-cache")
    .help("Build or update source resource conflict data")
    .req_flag(());
  let resource = positional::<String>("RESOURCE").optional();
  let source_dir = short('i')
    .long("input")
    .help("Source resource directory (default: asset/source)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/source"));
  let common = common::parser(PathBuf::from("asset/source-cache"));
  let exclusions = exclusions_parser();
  construct!(command, source_dir, common, exclusions, resource).map(
    |(_, source_dir, common, exclusions, resource)| GenerateSourceCache {
      source_dir,
      resource,
      exclusions,
      common,
    },
  )
}

pub(crate) fn exclusions_parser() -> impl Parser<Option<Vec<String>>> {
  long("exclude")
    .help("Comma-separated resource names or partial source-relative paths for this run")
    .argument::<String>("PATTERNS")
    .parse(|value| crate::core::source_cache::exclusions::parse_exclusions(&value))
    .optional()
}

/// Builds or reuses the source-derived conflict cache.
pub fn run(command: GenerateSourceCache) -> Result<(), Box<dyn std::error::Error>> {
  let build = crate::core::source_cache::BuildSourceCache {
    source_dir: command.source_dir,
    output_dir: command.common.output,
    vanilla_dir: command.common.vanilla,
    vanilla_cache_dir: command.common.vanilla_cache,
    resource: command.resource,
    exclusions: command.exclusions,
    force: command.common.force,
  };
  let rebuilt = build.run()?;
  if rebuilt {
    println!("Generated source cache.");
  } else {
    println!("Source cache is current; nothing to do.");
  }
  Ok(())
}
