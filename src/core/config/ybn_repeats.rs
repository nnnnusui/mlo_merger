//! The `[ybn_repeats]` table: leaving out of YBNs the polygons they hold twice.

use serde::{Deserialize, Serialize};
use std::io;

/// Whether merges leave out the polygons a YBN repeats, and which vanilla YBNs they deploy for
/// that alone.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[serde(default, deny_unknown_fields)]
pub struct YbnRepeatsConfig {
  /// Leaves repeats out of every YBN a merge builds, and builds the vanilla YBNs over
  /// [`Self::vanilla_min_share`]; off unless the config file turns it on, which the one shipped
  /// does.
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub enabled: bool,
  /// The least share of its polygons, from 0 to 1, a loaded vanilla YBN no resource changes
  /// must repeat for a merge to build it without them.
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub vanilla_min_share: f32,
}

impl Default for YbnRepeatsConfig {
  fn default() -> Self {
    Self {
      enabled: false,
      vanilla_min_share: 0.05,
    }
  }
}

impl YbnRepeatsConfig {
  /// Rejects a share outside 0 to 1.
  pub fn validate(&self) -> io::Result<()> {
    if !(0.0..=1.0).contains(&self.vanilla_min_share) {
      return Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "ybn_repeats.vanilla_min_share must be within 0 and 1",
      ));
    }
    Ok(())
  }

  /// Whether a vanilla YBN with `repeats` of `polygons` is built for them.
  pub fn builds_vanilla(
    &self,
    polygons: usize,
    repeats: usize,
  ) -> bool {
    self.enabled && repeats > 0 && repeats as f32 / polygons as f32 >= self.vanilla_min_share
  }
}
