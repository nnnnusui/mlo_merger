//! The `[dlc_activity]` table: corrections to which DLC archives the game loads.

use crate::platform::fs;
use std::{io, path::Path};

/// The `[dlc_activity]` table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DlcActivityConfig {
  /// Archives, as `content.xml` spells them with the platform directory left out
  /// (`dlc_mpheist4:/levels/gta5/lodlights.rpf`), that load in addition to the archives standing
  /// for the base game path they enable, rather than in place of them. Lowercase, sorted.
  pub additive_archives: Vec<String>,
}

impl DlcActivityConfig {
  /// Loads `[dlc_activity]` from optional TOML configuration; a missing file or table has no
  /// corrections.
  pub fn load(path: &Path) -> io::Result<Self> {
    let text = match fs::read_to_string(path) {
      Ok(text) => text,
      Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
      Err(error) => return Err(error),
    };
    Self::parse(&text)
      .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))
  }

  /// Parses `[dlc_activity]` from TOML configuration text.
  pub fn parse(text: &str) -> io::Result<Self> {
    let config = super::Config::parse(text)?;
    let mut additive_archives = config
      .dlc_activity
      .additive_archives
      .iter()
      .map(|path| crate::core::game_archive::dlc_content::archive_path(path))
      .collect::<Vec<_>>();
    if additive_archives.iter().any(|path| !path.contains(":/") || !path.ends_with(".rpf")) {
      return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "dlc_activity.additive_archives takes archive paths such as dlc_mpheist4:/levels/gta5/lodlights.rpf",
      ));
    }
    additive_archives.sort();
    additive_archives.dedup();
    Ok(Self {
      additive_archives,
    })
  }
}

/// The `[dlc_activity]` table as written.
#[derive(Debug, Default, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[serde(default, deny_unknown_fields)]
pub(super) struct DlcActivityTable {
  #[cfg_attr(feature = "typescript", specta(optional))]
  additive_archives: Vec<String>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn archives_are_spelled_as_dlc_activity_spells_them() {
    let config = DlcActivityConfig::parse(
      "[dlc_activity]\nadditive_archives = [\"dlc_mpHeist4:/%PLATFORM%/levels/gta5/lodlights.rpf\"]",
    )
    .unwrap();
    assert_eq!(config.additive_archives, ["dlc_mpheist4:/levels/gta5/lodlights.rpf"]);
    assert!(DlcActivityConfig::parse("").unwrap().additive_archives.is_empty());
    assert!(DlcActivityConfig::parse("[dlc_activity]\nadditive_archives = [\"x\"]").is_err());
  }
}
