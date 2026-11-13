pub mod gamebuild;
pub mod matching;

use serde::Deserialize;
use std::{
  collections::BTreeMap,
  io,
  path::{Path, PathBuf},
  sync::OnceLock,
};

static PATH: OnceLock<PathBuf> = OnceLock::new();

/// Sets the config file every operation reads for the rest of the process.
///
/// Only the first call takes effect; without one, the file is `asset/config.toml`.
pub fn set_path(path: PathBuf) {
  let _ = PATH.set(path);
}

/// The config file operations read: the one given to [`set_path`], or `asset/config.toml`.
pub fn path() -> &'static Path {
  PATH.get_or_init(|| PathBuf::from("asset/config.toml"))
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
}

impl Config {
  fn parse(text: &str) -> io::Result<Self> {
    toml::from_str(text).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
  }
}
