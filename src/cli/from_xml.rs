use std::path::PathBuf;

use bpaf::*;

use super::common::{self, CommonOptions};

/// Arguments for converting XML game files to native files.
#[derive(Debug, Clone)]
pub struct FromXml {
  /// XML input file or directory.
  pub input: PathBuf,
  /// Shared execution options.
  pub common: CommonOptions,
}

/// Parses `--from-xml <path>` arguments.
pub fn parser() -> impl Parser<FromXml> {
  let input = long("from-xml")
    .help("Convert an XML game file or directory to native files")
    .argument::<PathBuf>("PATH");
  let common = common::parser(PathBuf::from("."));
  construct!(input, common).map(|(input, common)| FromXml {
    input,
    common,
  })
}

/// Prints a mock invocation without converting files.
pub fn run_mock(command: FromXml) {
  println!("mock: --from-xml {:?} options={:?}", command.input, command.common);
}
