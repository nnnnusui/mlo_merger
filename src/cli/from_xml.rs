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

/// Converts the selected XML files through the core conversion workflow.
pub fn run(command: FromXml) -> Result<(), Box<dyn std::error::Error>> {
  let (converted, failed) = crate::core::conversion::from_xml(
    &command.input,
    &command.common.output,
    command.common.vanilla.exists().then_some(command.common.vanilla.as_path()),
  )?;
  println!("Converted {converted} XML files to binary; {failed} failed.");
  Ok(())
}
