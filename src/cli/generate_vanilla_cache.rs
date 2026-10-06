use std::path::PathBuf;

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
  crate::core::vanilla_cache::BuildVanillaCache {
    vanilla_dir: command.common.vanilla,
    output_dir: command.common.output,
    through_version: command.common.gamebuild,
    force: command.common.force,
  }
  .run()
}
