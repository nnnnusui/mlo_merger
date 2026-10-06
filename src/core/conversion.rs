use std::{error::Error, path::Path};

use super::format::gamefile::resource_convert;

/// Converts supported native game files to XML and reports conversion counts.
pub fn to_xml(
  input: &Path,
  output: &Path,
) -> Result<(usize, usize), Box<dyn Error>> {
  resource_convert::convert_files_to_xml(input, output)
}

/// Converts supported XML game files to native resources and reports conversion counts.
pub fn from_xml(
  input: &Path,
  output: &Path,
  schema_dir: Option<&Path>,
) -> Result<(usize, usize), Box<dyn Error>> {
  resource_convert::convert_files_from_xml(input, output, schema_dir)
}
