use std::path::PathBuf;

use bpaf::*;

use super::common::{self, CommonOptions};

/// Arguments for converting a native game file to XML.
#[derive(Debug, Clone)]
pub struct ToXml {
  /// Native input file or directory.
  pub input: PathBuf,
  /// Shared execution options.
  pub common: CommonOptions,
}

/// Parses `--to-xml <path>` arguments.
pub fn parser() -> impl Parser<ToXml> {
  let input = long("to-xml")
    .help("Convert a native game file or directory to XML")
    .argument::<PathBuf>("PATH");
  let common = common::parser(PathBuf::from("."));
  construct!(input, common).map(|(input, common)| ToXml {
    input,
    common,
  })
}

/// Prints a mock invocation without converting files.
pub fn run_mock(command: ToXml) {
  println!("mock: --to-xml {:?} options={:?}", command.input, command.common);
}
