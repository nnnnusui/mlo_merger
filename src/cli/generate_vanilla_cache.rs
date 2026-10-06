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

/// Prints a mock invocation without building cache data.
pub fn run_mock(command: GenerateVanillaCache) {
  super::common::run_mock("--generate-vanilla-cache", &command.common);
}
