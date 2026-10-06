use std::path::PathBuf;

use bpaf::*;

use super::common::{self, CommonOptions};

/// Arguments for comparing two stream files.
#[derive(Debug, Clone)]
pub struct GetDiff {
  /// Baseline input file.
  pub before: PathBuf,
  /// Modified input file.
  pub after: PathBuf,
  /// Shared execution options.
  pub common: CommonOptions,
}

/// Parses `--get-diff <a> <b>` arguments.
pub fn parser() -> impl Parser<GetDiff> {
  let command = long("get-diff").help("Compare two stream files").req_flag(());
  let common = common::parser(PathBuf::from("."));
  let before = positional::<PathBuf>("A");
  let after = positional::<PathBuf>("B");
  construct!(command, common, before, after).map(|(_, common, before, after)| GetDiff {
    before,
    after,
    common,
  })
}

/// Prints a mock invocation without comparing files.
pub fn run_mock(command: GetDiff) {
  println!(
    "mock: --get-diff {:?} {:?} options={:?}",
    command.before, command.after, command.common
  );
}
