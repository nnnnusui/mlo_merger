//! Matching tolerances loaded once per operation and scoped to its comparisons.

use crate::platform::fs;
use serde::{Deserialize, Serialize};
use std::{cell::Cell, io, path::Path};

/// Coordinate and scalar tolerances used by collision and map comparisons.
///
/// Tools round values differently when they save a file: CodeWalker and OpenIV, for example,
/// write the same position with slightly different digits. Without tolerances, values a
/// resource never touched would differ from vanilla only by that rounding and be detected, and
/// merged, as changes. Differences within these tolerances are treated as no change.
///
/// Position tolerances are named `*_position`. The names before that suffix was added (`ybn`,
/// `ymap_occlude_model`, `ymap_box_occluder`) are still read.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[serde(default, deny_unknown_fields)]
pub struct MatchTolerances {
  /// Inclusive per-axis tolerance for YBN polygon coordinates and radii.
  #[serde(alias = "ybn")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub ybn_position: f32,
  /// Exclusive tolerance for YMAP float values: entity positions, rotations, scales and LOD
  /// distances, and car generator, time-cycle modifier and distant LOD light values.
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub ymap: f32,
  /// Exclusive tolerance for the XY extents of YMAP occlude models when pairing them.
  #[serde(alias = "ymap_occlude_model")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub ymap_occlude_model_position: f32,
  /// Exclusive tolerance for YMAP box occluder centers, in stored units (a quarter metre).
  #[serde(alias = "ymap_box_occluder")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub ymap_box_occluder_position: i32,
  /// Inclusive tolerance for YMAP box occluder `i_cos_z`/`i_sin_z`, in stored units.
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub ymap_box_occluder_rotation: i32,
  /// Decimal places YMAP car generator positions are rounded to when pairing generators.
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub ymap_car_generator_position_decimals: u32,
  /// Inclusive per-axis world-space tolerance for YMAP grass instance positions, in metres.
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub ymap_grass_position: f32,
  /// Decimal places YMAP distant LOD light positions are rounded to when pairing lights.
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub ymap_lod_light_position_decimals: u32,
}

impl Default for MatchTolerances {
  fn default() -> Self {
    Self {
      ybn_position: 0.05,
      ymap: 0.001,
      ymap_occlude_model_position: 0.01,
      ymap_box_occluder_position: 1,
      ymap_box_occluder_rotation: 1,
      ymap_car_generator_position_decimals: 3,
      ymap_grass_position: 0.01,
      ymap_lod_light_position_decimals: 3,
    }
  }
}

impl MatchTolerances {
  /// [`Self::default`] recording every YMAP change however small, for diffs replayed to rebuild
  /// the YMAP they were taken to. Items still pair as by default, except occlude models, which
  /// never pair, since the reordering of their triangles is not recorded.
  pub fn ymap_exact() -> Self {
    Self {
      ymap: 0.0,
      ymap_occlude_model_position: 0.0,
      ymap_box_occluder_rotation: 0,
      ymap_grass_position: 0.0,
      ..Self::default()
    }
  }

  /// Loads optional TOML configuration, rejecting invalid matching tolerances.
  ///
  /// ```no_run
  /// let tolerance = mlo_merger::core::config::matching::MatchTolerances::load(
  ///   std::path::Path::new("asset/config.toml"),
  /// )?;
  /// assert!(tolerance.ybn_position > 0.0);
  /// # Ok::<(), std::io::Error>(())
  /// ```
  pub fn load(path: &Path) -> io::Result<Self> {
    let text = match fs::read_to_string(path) {
      Ok(text) => text,
      Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
      Err(error) => return Err(error),
    };
    Self::parse(&text)
      .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))
  }

  fn parse(text: &str) -> io::Result<Self> {
    let value = super::Config::parse(text)?.tolerance;
    for (name, tolerance) in [
      ("ybn_position", value.ybn_position),
      ("ymap", value.ymap),
      ("ymap_occlude_model_position", value.ymap_occlude_model_position),
      ("ymap_grass_position", value.ymap_grass_position),
    ] {
      if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err(io::Error::new(
          io::ErrorKind::InvalidData,
          format!("tolerance.{name} must be positive and finite"),
        ));
      }
    }
    if value.ymap_box_occluder_position <= 0 {
      return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "tolerance.ymap_box_occluder_position must be positive",
      ));
    }
    if value.ymap_box_occluder_rotation < 0 {
      return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "tolerance.ymap_box_occluder_rotation must not be negative",
      ));
    }
    for (name, decimals) in [
      ("ymap_car_generator_position_decimals", value.ymap_car_generator_position_decimals),
      ("ymap_lod_light_position_decimals", value.ymap_lod_light_position_decimals),
    ] {
      if decimals > 6 {
        return Err(io::Error::new(
          io::ErrorKind::InvalidData,
          format!("tolerance.{name} must be at most 6"),
        ));
      }
    }
    Ok(value)
  }
}

thread_local! {
  static ACTIVE: Cell<Option<MatchTolerances>> = const { Cell::new(None) };
}

pub(crate) fn current() -> MatchTolerances {
  ACTIVE.with(Cell::get).unwrap_or_default()
}

struct Scope(Option<MatchTolerances>);

impl Drop for Scope {
  fn drop(&mut self) {
    ACTIVE.with(|active| active.set(self.0));
  }
}

/// Keeps one immutable configuration snapshot for synchronous comparisons without
/// mutating global equality behavior in other threads or nested operations.
pub(crate) fn with_tolerances<T>(
  value: MatchTolerances,
  operation: impl FnOnce() -> T,
) -> T {
  let _scope = Scope(ACTIVE.with(|active| active.replace(Some(value))));
  operation()
}

/// Nested format handlers inherit the outer operation's validated snapshot.
pub(crate) fn with_config<T>(operation: impl FnOnce() -> T) -> io::Result<T> {
  if ACTIVE.with(Cell::get).is_some() {
    return Ok(operation());
  }
  Ok(with_tolerances(MatchTolerances::load(super::path())?, operation))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn defaults_partial_config_and_invalid_values() {
    assert_eq!(MatchTolerances::parse("").unwrap(), MatchTolerances::default());
    let configured =
      MatchTolerances::parse("[tolerance]\nybn_position = 0.025\nymap = 0.002").unwrap();
    assert_eq!(configured.ybn_position, 0.025);
    assert_eq!(configured.ymap, 0.002);
    assert_eq!(configured.ymap_occlude_model_position, 0.01);
    assert_eq!(configured.ymap_box_occluder_rotation, 1);
    assert_eq!(configured.ymap_car_generator_position_decimals, 3);
    assert_eq!(configured.ymap_lod_light_position_decimals, 3);
    assert_eq!(configured.ymap_grass_position, 0.01);
    let exact = MatchTolerances::parse(
      "[tolerance]\nymap_box_occluder_rotation = 0\nymap_car_generator_position_decimals = 0",
    )
    .unwrap();
    assert_eq!(exact.ymap_box_occluder_rotation, 0);
    assert_eq!(exact.ymap_car_generator_position_decimals, 0);
    for text in [
      "[tolerance]\nybn_position = 0",
      "[tolerance]\nybn_position = nan",
      "[tolerance]\nymap = inf",
      "[tolerance]\nymap_occlude_model_position = -0.01",
      "[tolerance]\nymap_box_occluder_position = 0",
      "[tolerance]\nymap_box_occluder_rotation = -1",
      "[tolerance]\nymap_car_generator_position_decimals = 7",
      "[tolerance]\nymap_lod_light_position_decimals = 7",
      "[tolerance]\nymap_grass_position = 0",
      "[tolerance]\nunknown = 0.1",
      "not valid toml",
    ] {
      assert!(MatchTolerances::parse(text).is_err(), "{text}");
    }
    let missing =
      std::env::temp_dir().join(format!("missing_match_config_{}.toml", std::process::id()));
    assert_eq!(MatchTolerances::load(&missing).unwrap(), MatchTolerances::default());
  }

  #[test]
  fn position_tolerances_are_read_under_their_former_names_too() {
    let former = MatchTolerances::parse(
      "[tolerance]\nybn = 0.025\nymap_occlude_model = 0.02\nymap_box_occluder = 2",
    )
    .unwrap();
    assert_eq!(former.ybn_position, 0.025);
    assert_eq!(former.ymap_occlude_model_position, 0.02);
    assert_eq!(former.ymap_box_occluder_position, 2);
    assert!(MatchTolerances::parse("[tolerance]\nybn = 0").is_err());
    assert!(MatchTolerances::parse("[tolerance]\nybn = 0.1\nybn_position = 0.1").is_err());
  }

  #[test]
  fn config_file_reads_values_and_reports_invalid_paths() {
    let path =
      std::env::temp_dir().join(format!("match_config_values_{}.toml", std::process::id()));
    fs::write(&path, "[tolerance]\nybn_position = 0.025\nymap = 0.002").unwrap();
    let loaded = MatchTolerances::load(&path).unwrap();
    assert_eq!(loaded.ybn_position, 0.025);
    assert_eq!(loaded.ymap, 0.002);
    assert_eq!(loaded.ymap_occlude_model_position, 0.01);
    fs::write(&path, "[tolerance]\nybn_position = 0.0").unwrap();
    assert!(MatchTolerances::load(&path).unwrap_err().to_string().contains(path.to_str().unwrap()));
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn operation_snapshot_is_nested_thread_local_and_restored_after_unwinding() {
    let defaults = current();
    let configured = MatchTolerances {
      ybn_position: 0.025,
      ..defaults
    };
    with_tolerances(configured, || {
      assert_eq!(current(), configured);
      with_config(|| assert_eq!(current(), configured)).unwrap();
      assert_eq!(std::thread::spawn(current).join().unwrap(), defaults);
      with_tolerances(defaults, || assert_eq!(current(), defaults));
      assert_eq!(current(), configured);
    });
    assert_eq!(current(), defaults);
    let _ = std::panic::catch_unwind(|| with_tolerances(configured, || panic!("test unwind")));
    assert_eq!(current(), defaults);
  }
}
