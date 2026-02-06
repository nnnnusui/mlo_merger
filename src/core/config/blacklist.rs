use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlacklistConfig {
  #[serde(default)]
  pub occlude_models: Vec<OccludeModelBlacklist>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OccludeModelBlacklist {
  #[serde(default)]
  pub bmin: Option<[f32; 3]>,
  #[serde(default)]
  pub bmax: Option<[f32; 3]>,
}

impl BlacklistConfig {
  pub fn from_file(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let config: BlacklistConfig = toml::from_str(&content)?;
    Ok(config)
  }

  pub fn is_occlude_model_blacklisted(
    &self,
    bmin: &crate::core::common::position::Position,
    bmax: &crate::core::common::position::Position,
  ) -> bool {
    const EPSILON: f32 = 0.01;

    self.occlude_models.iter().any(|blacklist| {
      if let Some(blacklisted_bmin) = &blacklist.bmin {
        let matches_bmin = (bmin.x - blacklisted_bmin[0]).abs() < EPSILON
          && (bmin.y - blacklisted_bmin[1]).abs() < EPSILON
          && (bmin.z - blacklisted_bmin[2]).abs() < EPSILON;
        if matches_bmin {
          return true;
        }
      }

      if let Some(blacklisted_bmax) = &blacklist.bmax {
        let matches_bmax = (bmax.x - blacklisted_bmax[0]).abs() < EPSILON
          && (bmax.y - blacklisted_bmax[1]).abs() < EPSILON
          && (bmax.z - blacklisted_bmax[2]).abs() < EPSILON;
        if matches_bmax {
          return true;
        }
      }

      false
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::common::position::Position;

  #[test]
  fn test_is_occlude_model_blacklisted() {
    let config = BlacklistConfig {
      occlude_models: vec![
        OccludeModelBlacklist {
          bmin: Some([-407.08, -2047.73, 14.22]),
          bmax: None,
        },
        OccludeModelBlacklist {
          bmin: None,
          bmax: Some([100.0, 200.0, 300.0]),
        },
      ],
    };

    // Test matching bmin
    let bmin1 = Position {
      x: -407.08,
      y: -2047.73,
      z: 14.22,
    };
    let bmax1 = Position {
      x: 0.0,
      y: 0.0,
      z: 0.0,
    };
    assert!(config.is_occlude_model_blacklisted(&bmin1, &bmax1));

    // Test matching bmax
    let bmin2 = Position {
      x: 0.0,
      y: 0.0,
      z: 0.0,
    };
    let bmax2 = Position {
      x: 100.0,
      y: 200.0,
      z: 300.0,
    };
    assert!(config.is_occlude_model_blacklisted(&bmin2, &bmax2));

    // Test not matching
    let bmin3 = Position {
      x: 50.0,
      y: 50.0,
      z: 50.0,
    };
    let bmax3 = Position {
      x: 150.0,
      y: 150.0,
      z: 150.0,
    };
    assert!(!config.is_occlude_model_blacklisted(&bmin3, &bmax3));
  }
}
