use std::path::{Path, PathBuf};

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
pub fn parser(asset: &Path) -> impl Parser<ToXml> + use<> {
  options(asset)
    .to_options()
    .descr("Convert a native game file or directory to XML")
    .command("--to-xml")
}

fn options(asset: &Path) -> impl Parser<ToXml> + use<> {
  let input = positional::<PathBuf>("PATH").help("File or directory to convert");
  let common = common::parser(asset, PathBuf::from("."));
  construct!(common, input).map(|(common, input)| ToXml {
    input,
    common,
  })
}

/// Converts the selected native files through the core conversion workflow.
pub fn run(command: ToXml) -> Result<String, Box<dyn std::error::Error>> {
  let (converted, failed) = crate::core::conversion::to_xml(
    &command.input,
    &command.common.output,
    &command.common.vanilla,
  )?;
  Ok(format!("Converted {converted} files to XML; {failed} failed.\n"))
}
