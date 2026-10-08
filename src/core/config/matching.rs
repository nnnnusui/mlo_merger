//! Matching tolerances loaded once per operation and scoped to its comparisons.

use serde::{Deserialize, Serialize};
use std::{cell::Cell, fs, io, path::Path};

/// Coordinate and scalar tolerances used by collision and map comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[serde(default, deny_unknown_fields)]
pub struct MatchTolerances {
  /// Inclusive per-axis polygon coordinate and radius tolerance for YBNs.
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub ybn: f32,
  /// Exclusive scalar tolerance for YMAP car generators and LOD lights.
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub ymap: f32,
  /// Exclusive XY extent tolerance for YMAP occlude-model matching.
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub ymap_occlude_model: f32,
  /// Exclusive integer center tolerance for YMAP box occluders, in stored units.
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub ymap_box_occluder: i32,
}

impl Default for MatchTolerances {
  fn default() -> Self {
    Self {
      ybn: 0.05,
      ymap: 0.001,
      ymap_occlude_model: 0.01,
      ymap_box_occluder: 1,
    }
  }
}

#[derive(Default, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[cfg_attr(feature = "typescript", specta(rename = "MatchingConfig"))]
#[serde(default, deny_unknown_fields)]
struct Config {
  #[cfg_attr(feature = "typescript", specta(optional))]
  tolerance: MatchTolerances,
}

impl MatchTolerances {
  /// Loads optional TOML configuration, rejecting invalid matching tolerances.
  ///
  /// ```no_run
  /// let tolerance = mlo_merger::core::config::matching::MatchTolerances::load(
  ///   std::path::Path::new("asset/config.toml"),
  /// )?;
  /// assert!(tolerance.ybn > 0.0);
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
    let config: Config =
      toml::from_str(text).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let value = config.tolerance;
    for (name, tolerance) in
      [("ybn", value.ybn), ("ymap", value.ymap), ("ymap_occlude_model", value.ymap_occlude_model)]
    {
      if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err(io::Error::new(
          io::ErrorKind::InvalidData,
          format!("tolerance.{name} must be positive and finite"),
        ));
      }
    }
    if value.ymap_box_occluder <= 0 {
      return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "tolerance.ymap_box_occluder must be positive",
      ));
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
  Ok(with_tolerances(MatchTolerances::load(Path::new("asset/config.toml"))?, operation))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn defaults_partial_config_and_invalid_values() {
    assert_eq!(MatchTolerances::parse("").unwrap(), MatchTolerances::default());
    let configured = MatchTolerances::parse("[tolerance]\nybn = 0.025\nymap = 0.002").unwrap();
    assert_eq!(configured.ybn, 0.025);
    assert_eq!(configured.ymap, 0.002);
    assert_eq!(configured.ymap_occlude_model, 0.01);
    for text in [
      "[tolerance]\nybn = 0",
      "[tolerance]\nybn = nan",
      "[tolerance]\nymap = inf",
      "[tolerance]\nymap_occlude_model = -0.01",
      "[tolerance]\nymap_box_occluder = 0",
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
  fn config_file_reads_values_and_reports_invalid_paths() {
    let path =
      std::env::temp_dir().join(format!("match_config_values_{}.toml", std::process::id()));
    fs::write(&path, "[tolerance]\nybn = 0.025\nymap = 0.002").unwrap();
    let loaded = MatchTolerances::load(&path).unwrap();
    assert_eq!(loaded.ybn, 0.025);
    assert_eq!(loaded.ymap, 0.002);
    assert_eq!(loaded.ymap_occlude_model, 0.01);
    fs::write(&path, "[tolerance]\nybn = 0.0").unwrap();
    assert!(MatchTolerances::load(&path).unwrap_err().to_string().contains(path.to_str().unwrap()));
    fs::remove_file(path).unwrap();
  }

  #[test]
  fn operation_snapshot_is_nested_thread_local_and_restored_after_unwinding() {
    let defaults = current();
    let configured = MatchTolerances {
      ybn: 0.025,
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
