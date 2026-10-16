use std::path::{Path, PathBuf};

use bpaf::*;

use super::common::{self, CommonOptions};

/// Arguments for deriving indexes and latest files from vanilla archives.
#[derive(Debug, Clone)]
pub struct GenerateVanillaCache {
  /// Shared execution options.
  pub common: CommonOptions,
}

/// Parses `--generate-vanilla-cache` arguments.
pub fn parser() -> impl Parser<GenerateVanillaCache> {
  let command = long("generate-vanilla-cache")
    .help("Build derived vanilla indexes and latest stream files")
    .req_flag(());
  let common = common::parser(PathBuf::from("asset/vanilla-cache"));
  construct!(command, common).map(|(_, common)| GenerateVanillaCache {
    common,
  })
}

/// Builds the derived vanilla cache from versioned raw vanilla files.
pub fn run(command: GenerateVanillaCache) -> Result<(), Box<dyn std::error::Error>> {
  let result = crate::core::vanilla_cache::BuildVanillaCache {
    vanilla_dir: command.common.vanilla,
    output_dir: command.common.output,
    through_version: command.common.gamebuild,
    force: command.common.force,
  }
  .run()?;
  tracing::info!("{}", result.summary());
  Ok(())
}

/// Builds the latest vanilla cache when its inputs changed, as a prerequisite of later steps.
pub(crate) fn update(
  vanilla_dir: &Path,
  vanilla_cache_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
  crate::core::vanilla_cache::BuildVanillaCache {
    vanilla_dir: vanilla_dir.to_path_buf(),
    output_dir: vanilla_cache_dir.to_path_buf(),
    through_version: None,
    force: false,
  }
  .run()?;
  Ok(())
}
