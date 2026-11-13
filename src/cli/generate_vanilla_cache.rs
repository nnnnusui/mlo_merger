use std::path::Path;

use bpaf::*;

use super::common::{self, CommonOptions};

/// Arguments for deriving indexes and latest files from vanilla archives.
#[derive(Debug, Clone)]
pub struct GenerateVanillaCache {
  /// Shared execution options.
  pub common: CommonOptions,
}

/// Parses `--generate-vanilla-cache` arguments.
pub fn parser(asset: &Path) -> impl Parser<GenerateVanillaCache> + use<> {
  let command = long("generate-vanilla-cache")
    .help("Build derived vanilla indexes and latest stream files")
    .req_flag(());
  let common = common::parser(asset, asset.join("vanilla-cache"));
  construct!(command, common).map(|(_, common)| GenerateVanillaCache {
    common,
  })
}

/// Builds the derived vanilla cache from versioned raw vanilla files; prints nothing.
pub fn run(command: GenerateVanillaCache) -> Result<String, Box<dyn std::error::Error>> {
  let result = crate::core::vanilla_cache::BuildVanillaCache {
    vanilla_dir: command.common.vanilla,
    output_dir: command.common.output,
    gamebuild: common::gamebuild(command.common.gamebuild.as_deref())?,
    force: command.common.force,
  }
  .run()?;
  tracing::info!("{}", result.summary());
  Ok(String::new())
}

/// Builds the vanilla cache for `--gamebuild` (the last stage when absent) when its inputs
/// changed, as a prerequisite of later steps.
pub(crate) fn update(common: &CommonOptions) -> Result<(), Box<dyn std::error::Error>> {
  crate::core::vanilla_cache::BuildVanillaCache {
    vanilla_dir: common.vanilla.clone(),
    output_dir: common.vanilla_cache.clone(),
    gamebuild: common::gamebuild(common.gamebuild.as_deref())?,
    force: false,
  }
  .run()?;
  Ok(())
}
