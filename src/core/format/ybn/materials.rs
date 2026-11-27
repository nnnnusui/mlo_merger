//! The names of collision surface types, from the game's `materials.dat`.
//!
//! A YBN material names its surface type by the index of its entry in
//! `common:/data/materials/materials.dat`, which the title update replaces.

use serde::{Deserialize, Serialize};

/// The surface types of a `materials.dat`, by index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct SurfaceMaterials {
  /// The file the names were read from.
  pub source: String,
  /// SHA-256 of that file.
  pub sha256: String,
  /// Each surface type's name, at its index.
  pub names: Vec<String>,
}

/// The surface type names of a `materials.dat`, by index, read as CodeWalker reads them: after
/// the version on the first line, each line of at least 20 characters that is not a `#` comment
/// and holds at least 10 columns, separated by tabs or spaces, is one surface type, named by its
/// first column.
pub fn parse_materials_dat(text: &str) -> Vec<String> {
  text
    .split('\n')
    .skip(1)
    .filter(|line| line.len() >= 20 && !line.starts_with('#'))
    .filter_map(|line| {
      let columns = line.split(['\t', ' ']).map(str::trim).filter(|column| !column.is_empty());
      let columns = columns.collect::<Vec<_>>();
      (columns.len() >= 10).then(|| columns[0].to_owned())
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn surface_types_are_the_first_column_of_their_lines_in_order() {
    let row =
      |name: &str| format!("{name}\t\t\tFILTER\tFX\tDEFAULT\t-\t-\t1.0\t0.1\t1750.0\t1.0\r");
    let text = [
      "12.00\r".to_owned(),
      "# MATERIAL INFORMATION\r".to_owned(),
      "\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\t\r".to_owned(),
      row("DEFAULT"),
      "# CONCRETE\r".to_owned(),
      row("CONCRETE"),
      "SHORT LINE\r".to_owned(),
      row("STUNT_RAMP_SURFACE"),
    ]
    .join("\n");
    assert_eq!(parse_materials_dat(&text), ["DEFAULT", "CONCRETE", "STUNT_RAMP_SURFACE"]);
  }
}
