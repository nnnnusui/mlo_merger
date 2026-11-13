use crate::platform::fs;
use std::path::{Path, PathBuf};

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
pub fn parser(asset: &Path) -> impl Parser<FromXml> + use<> {
  options(asset)
    .to_options()
    .descr("Convert an XML game file or directory to native files")
    .command("--from-xml")
}

fn options(asset: &Path) -> impl Parser<FromXml> + use<> {
  let input = positional::<PathBuf>("PATH").help("File or directory to convert");
  let common = common::parser(asset, PathBuf::from("."));
  construct!(common, input).map(|(common, input)| FromXml {
    input,
    common,
  })
}

/// Converts the selected XML files through the core conversion workflow.
pub fn run(command: FromXml) -> Result<String, Box<dyn std::error::Error>> {
  let (converted, failed) = crate::core::conversion::from_xml(
    &command.input,
    &command.common.output,
    fs::exists(&command.common.vanilla)
      .unwrap_or(false)
      .then_some(command.common.vanilla.as_path()),
  )?;
  Ok(format!("Converted {converted} XML files to binary; {failed} failed.\n"))
}
