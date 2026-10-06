use std::path::PathBuf;

use bpaf::*;

use super::common::{self, CommonOptions};

/// Arguments for indexing source resources and their vanilla conflicts.
#[derive(Debug, Clone)]
pub struct GenerateSourceCache {
  /// Source resource directory.
  pub source_dir: PathBuf,
  /// Shared execution options.
  pub common: CommonOptions,
}

/// Parses `--generate-source-cache` arguments.
pub fn parser() -> impl Parser<GenerateSourceCache> {
  let command = long("generate-source-cache")
    .help("Build or update source resource conflict data")
    .req_flag(());
  let source_dir = short('i')
    .long("input")
    .help("Source resource directory (default: asset/source)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/source"));
  let common = common::parser(PathBuf::from("asset/source-cache"));
  construct!(command, source_dir, common).map(|(_, source_dir, common)| GenerateSourceCache {
    source_dir,
    common,
  })
}

/// Prints a mock invocation without scanning resources.
pub fn run_mock(command: GenerateSourceCache) {
  println!(
    "mock: --generate-source-cache source_dir={:?} options={:?}",
    command.source_dir, command.common
  );
}
