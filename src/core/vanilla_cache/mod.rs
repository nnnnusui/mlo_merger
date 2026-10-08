//! Derived latest-file and YMAP relationship indexes for raw vanilla history.

use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  io::{BufReader, BufWriter, Write},
  path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use crate::core::vanilla::VanillaCacheManifest;
use crate::core::{
  format::gamefile::{
    meta_resource::{MetaResource, jenk_hash},
    resource_file::Rsc7Resource,
  },
  format::ymap::diff::reference_hash,
  vanilla::{CachedFile, read_ymap},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const CACHE_SCHEMA: u32 = 2;

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
struct DerivedManifest {
  format_version: u32,
  vanilla_manifest_sha256: String,
  latest_version: String,
  vanilla_input_timestamps: BTreeMap<String, SourceTimestamp>,
  files: BTreeMap<String, LatestFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SourceTimestamp {
  modified_seconds: u64,
  modified_nanos: u32,
  size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LatestFile {
  version: String,
  sha256: String,
  object: String,
  vanilla_object: String,
  source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct YmapRelationshipIndex {
  format_version: u32,
  vanilla_manifest_sha256: String,
  version: String,
  children_by_parent_hash: BTreeMap<String, Vec<String>>,
}

mod build;
mod filesystem;
mod freshness;
mod relationships;
mod source;

use filesystem::*;
use freshness::cache_is_current;
use relationships::children_by_parent_hash;
pub(crate) use relationships::ymap_parent_hash;
use source::*;

#[cfg(test)]
mod tests;
