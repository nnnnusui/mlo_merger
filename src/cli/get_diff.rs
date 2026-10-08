use std::path::PathBuf;

use bpaf::*;

use super::common::{self, CommonOptions};

/// Arguments for generating a vanilla-relative diff or comparing two explicit stream files.
#[derive(Debug, Clone)]
pub struct GetDiff {
  /// Source input file.
  pub before: PathBuf,
  /// Optional explicit comparison target; absent means latest vanilla baseline.
  pub after: Option<PathBuf>,
  /// Shared execution options.
  pub common: CommonOptions,
}

/// Parses `--get-diff <source> [target]` arguments.
pub fn parser() -> impl Parser<GetDiff> {
  let command = long("get-diff")
    .help("Generate a YMAP diff JSON against the latest vanilla baseline")
    .req_flag(());
  let common = common::parser(PathBuf::from("."));
  let before = positional::<PathBuf>("SOURCE");
  let after = positional::<PathBuf>("TARGET").optional();
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

/// Generates a vanilla-relative diff for a single YMAP input.
pub fn run(command: GetDiff) -> Result<(), Box<dyn std::error::Error>> {
  if command.after.is_some() {
    run_mock(command);
    return Ok(());
  }
  let output = crate::core::get_diff::generate_ymap_diff(
    &command.before,
    &command.common.vanilla_cache,
    &command.common.output,
  )?;
  for path in output {
    println!("Wrote YMAP diff to {}", path.display());
  }
  Ok(())
}
