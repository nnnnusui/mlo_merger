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

/// Converts the selected native files through the core conversion workflow.
pub fn run(command: ToXml) -> Result<(), Box<dyn std::error::Error>> {
  let (converted, failed) =
    crate::core::conversion::to_xml(&command.input, &command.common.output)?;
  println!("Converted {converted} files to XML; {failed} failed.");
  Ok(())
}
