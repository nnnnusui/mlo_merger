pub mod dlc_activity;
pub mod gamebuild;
pub mod matching;
pub mod ybn_repeats;

use crate::core::format::ybn::diff::polygons::GeometryLimits;
use serde::Deserialize;
use std::{
  collections::BTreeMap,
  io,
  path::PathBuf,
  sync::{Mutex, PoisonError},
};

static PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Sets the config file operations read from now on; without it, the file is
/// `asset/config.toml`.
pub fn set_path(path: PathBuf) {
  *PATH.lock().unwrap_or_else(PoisonError::into_inner) = Some(path);
}

/// The config file operations read: the one last given to [`set_path`], or
/// `asset/config.toml`.
pub fn path() -> PathBuf {
  PATH
    .lock()
    .unwrap_or_else(PoisonError::into_inner)
    .clone()
    .unwrap_or_else(|| PathBuf::from("asset/config.toml"))
}

/// The config file ([`path`]), each table optional.
#[derive(Default, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[cfg_attr(feature = "typescript", specta(rename = "MatchingConfig"))]
#[serde(default, deny_unknown_fields)]
struct Config {
  #[cfg_attr(feature = "typescript", specta(optional))]
  tolerance: matching::MatchTolerances,
  /// FiveM game builds and the DLC names that identify them, keyed by build number.
  #[cfg_attr(feature = "typescript", specta(optional))]
  gamebuild: BTreeMap<String, Vec<String>>,
  /// The SHA-1 of each FiveM game build's `update/update.rpf`, keyed by build number.
  #[cfg_attr(feature = "typescript", specta(optional))]
  update_rpf: BTreeMap<String, String>,
  /// Corrections to which DLC archives the game loads.
  #[cfg_attr(feature = "typescript", specta(optional))]
  dlc_activity: dlc_activity::DlcActivityTable,
  /// How merged YBN geometry children are filled.
  #[cfg_attr(feature = "typescript", specta(optional))]
  ybn: GeometryLimits,
  /// Leaving out of YBNs the polygons they hold twice.
  #[cfg_attr(feature = "typescript", specta(optional))]
  ybn_repeats: ybn_repeats::YbnRepeatsConfig,
}

impl Config {
  fn parse(text: &str) -> io::Result<Self> {
    toml::from_str(text).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
  }
}

/// The `[ybn]` limits of the config file ([`path`]), the defaults when it is missing.
pub fn geometry_limits() -> io::Result<GeometryLimits> {
  let path = path();
  let text = match crate::platform::fs::read_to_string(&path) {
    Ok(text) => text,
    Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(GeometryLimits::default()),
    Err(error) => return Err(error),
  };
  let limits = Config::parse(&text)
    .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))?
    .ybn;
  limits.validate()?;
  Ok(limits)
}

/// The `[ybn_repeats]` table of the config file ([`path`]), the defaults when it is missing.
pub fn ybn_repeats() -> io::Result<ybn_repeats::YbnRepeatsConfig> {
  let path = path();
  let text = match crate::platform::fs::read_to_string(&path) {
    Ok(text) => text,
    Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Default::default()),
    Err(error) => return Err(error),
  };
  let repeats = Config::parse(&text)
    .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))?
    .ybn_repeats;
  repeats.validate()?;
  Ok(repeats)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn ybn_repeats_default_to_disabled_and_parse() {
    let defaults = Config::parse("").unwrap().ybn_repeats;
    assert_eq!(defaults, Default::default());
    assert!(!defaults.enabled && !defaults.builds_vanilla(100, 50));
    let configured =
      Config::parse("[ybn_repeats]\nenabled = true\nvanilla_min_share = 0.2").unwrap().ybn_repeats;
    assert!(configured.enabled);
    assert_eq!(configured.vanilla_min_share, 0.2);
    assert!(Config::parse("[ybn_repeats]\nshare = 1").is_err());
    let enabled = Config::parse("[ybn_repeats]\nenabled = true").unwrap().ybn_repeats;
    assert_eq!(enabled.vanilla_min_share, 0.05);
    assert!(enabled.builds_vanilla(100, 5) && !enabled.builds_vanilla(100, 4));
    assert!(!enabled.builds_vanilla(100, 0));
    assert!(!configured.builds_vanilla(100, 19) && configured.builds_vanilla(100, 20));
    assert!(
      ybn_repeats::YbnRepeatsConfig {
        vanilla_min_share: 1.5,
        ..defaults
      }
      .validate()
      .is_err()
    );
  }
}
