use std::path::PathBuf;

use bpaf::*;

use super::common::{self, CommonOptions};

/// Arguments for merging cached source conflicts into vanilla stream files.
#[derive(Debug, Clone)]
pub struct Merge {
  /// Shared execution options.
  pub common: CommonOptions,
}

/// Parses `--merge` arguments.
pub fn parser() -> impl Parser<Merge> {
  let command = long("merge").help("Merge source conflicts into vanilla stream files").req_flag(());
  let common = common::parser(PathBuf::from("asset/merged"));
  construct!(command, common).map(|(_, common)| Merge {
    common,
  })
}

/// Prints a mock invocation without merging files.
pub fn run_mock(command: Merge) {
  super::common::run_mock("--merge", &command.common);
}
