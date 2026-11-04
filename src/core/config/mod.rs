pub mod gamebuild;
pub mod matching;

use serde::Deserialize;
use std::{collections::BTreeMap, io};

/// `asset/config.toml`, each table optional.
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
