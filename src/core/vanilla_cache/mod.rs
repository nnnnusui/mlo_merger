//! Derived latest-file and YMAP relationship indexes for raw vanilla history.

use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  io::{BufReader, BufWriter, Write},
  path::{Component, Path, PathBuf},
};

pub use crate::core::vanilla::VanillaCacheManifest;
use crate::core::{
  format::gamefile::{
    meta::{MetaResource, jenk_hash},
    rsc7::Rsc7Resource,
  },
  format::ymap::diff::reference_hash,
  vanilla::{CachedFile, read_ymap},
};
use serde::{Deserialize, Serialize};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const CACHE_SCHEMA: u32 = 2;

/// Outcome of building or reusing a derived vanilla cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaCacheResult {
  /// Final vanilla stage selected for this cache.
  pub latest_version: String,
  /// Whether the cache was rebuilt and published.
  pub rebuilt: bool,
}

impl VanillaCacheResult {
  /// Returns a human-readable summary without emitting a log record.
  ///
  /// ```
  /// use mlo_merger::core::vanilla_cache::VanillaCacheResult;
  /// let result = VanillaCacheResult { latest_version: "0000-base".into(), rebuilt: false };
  /// assert_eq!(result.summary(), "Vanilla derived cache is current through 0000-base");
  /// ```
  pub fn summary(&self) -> String {
    if self.rebuilt {
      format!("Built vanilla derived cache through {}", self.latest_version)
    } else {
      format!("Vanilla derived cache is current through {}", self.latest_version)
    }
  }
}

/// Builds a derived cache from versioned native files in the vanilla archive.
#[derive(Debug, Clone)]
pub struct BuildVanillaCache {
  /// Directory containing the raw vanilla cache manifest and stage artifacts.
  pub vanilla_dir: PathBuf,
  /// Destination for latest files, relationship indexes and derived metadata.
  pub output_dir: PathBuf,
  /// Optional final vanilla stage ID, stage label, or numeric stage position.
  pub through_version: Option<String>,
  /// Rebuilds this stage even when its inputs and outputs are current.
  pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
struct DerivedManifest {
  format_version: u32,
  vanilla_manifest_sha256: String,
  latest_version: String,
  vanilla_input_timestamps: BTreeMap<String, SourceTimestamp>,
  files: BTreeMap<String, LatestFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
struct SourceTimestamp {
  modified_seconds: u64,
  modified_nanos: u32,
  size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
struct LatestFile {
  version: String,
  sha256: String,
  object: String,
  vanilla_object: String,
  source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(crate) struct YmapRelationshipIndex {
  pub(crate) format_version: u32,
  pub(crate) vanilla_manifest_sha256: String,
  pub(crate) version: String,
  pub(crate) children_by_parent_hash: BTreeMap<String, Vec<String>>,
}

mod build;
mod filesystem;
mod freshness;
mod read;
mod relationships;
mod source;

use filesystem::*;
use freshness::cache_is_current;
pub(crate) use read::{DerivedVanillaFile, LoadedCache, load_cache};
use relationships::children_by_parent_hash;
pub(crate) use relationships::ymap_parent_hash;
use source::*;

#[cfg(test)]
mod tests;
