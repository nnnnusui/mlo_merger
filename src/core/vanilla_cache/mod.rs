//! Derived latest-file and YMAP relationship indexes, and YMAP version diffs, for raw vanilla
//! history.
//!
//! The latest files are not copied or linked: `cache_info.json` names the vanilla archive and
//! each latest file's path in it, and readers resolve them through [`latest_files`].

use crate::platform::fs;
use std::{
  collections::{BTreeMap, BTreeSet},
  io::{BufReader, BufWriter, Write},
  path::{Component, Path, PathBuf},
};

use crate::common::changes::StoreChanges;
pub use crate::core::vanilla::VanillaCacheManifest;
use crate::core::{
  config::gamebuild::GameBuild,
  format::gamefile::{
    meta::{MetaResource, jenk_hash},
    rsc7::Rsc7Resource,
  },
  format::ymap::reference_hash,
  vanilla::{CachedFile, read_ymap},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub(crate) const CACHE_SCHEMA: u32 = 5;

/// Outcome of building or reusing a derived vanilla cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaCacheResult {
  /// Final vanilla stage selected for this cache.
  pub latest_version: String,
  /// Whether the cache was rebuilt and published.
  pub rebuilt: bool,
  /// Latest files the rebuild added, changed or removed, relative to the vanilla archive.
  pub changes: StoreChanges,
}

impl VanillaCacheResult {
  /// Returns a human-readable summary without emitting a log record.
  ///
  /// ```
  /// use mlo_merger::core::vanilla_cache::VanillaCacheResult;
  /// let result = VanillaCacheResult {
  ///   latest_version: "0000-base".into(), rebuilt: false, changes: Default::default(),
  /// };
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
  /// Game build whose last stage the latest files reflect; `None` is the last stage.
  pub gamebuild: Option<GameBuild>,
  /// Rebuilds this stage even when its inputs and outputs are current.
  pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
struct DerivedManifest {
  format_version: u32,
  /// The vanilla archive the latest files are in, relative to the cache directory when they
  /// share a root, with `/` separators.
  vanilla_dir: String,
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
  /// The file's path in the vanilla archive, such as `0000-base/ymap/{file name}`.
  object: String,
  source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(crate) struct YmapRelationshipIndex {
  pub(crate) format_version: u32,
  pub(crate) vanilla_manifest_sha256: String,
  pub(crate) version: String,
  /// Child YMAP names keyed by their parent's file name (`hash_XXXXXXXX` when unresolved).
  pub(crate) children_by_parent: BTreeMap<String, Vec<String>>,
}

mod build;
mod check;
mod filesystem;
mod freshness;
pub(crate) mod generations;
mod read;
mod relationships;
mod source;
mod ymap_diffs;

pub(crate) use check::check;
use filesystem::*;
use freshness::{cache_is_current, previous_manifest};
use generations::write_generations;
pub(crate) use read::archive_dir;
pub(crate) use read::{DerivedVanillaFile, LoadedCache, load_cache};
pub use read::{VanillaLatestFile, cache_info_path, latest_files};
use relationships::children_by_parent;
pub(crate) use relationships::ymap_parent_hash;
use source::*;
pub use ymap_diffs::{VanillaYmapDiff, VanillaYmapHistory, VanillaYmapVersion, ymap_history};

/// The vanilla stage the cache in `cache_dir` was built for, unless the cache is missing or
/// unreadable.
pub(crate) fn built_version(cache_dir: &Path) -> Option<String> {
  read::read_derived(cache_dir).ok().map(|loaded| loaded.manifest.latest_version.clone())
}

/// The vanilla stage the cache targets for `gamebuild`: its last stage, or the last stage of
/// the archive without one.
pub(crate) fn target_version(
  manifest: &VanillaCacheManifest,
  gamebuild: Option<&GameBuild>,
) -> Result<String> {
  Ok(manifest.versions[resolve_through_version(manifest, gamebuild)?].id.clone())
}
// Generation is disabled; see `BuildVanillaCache::run_cache`.
// use ymap_diffs::{write_ymap_diffs, ymap_diffs_are_current};

#[cfg(test)]
mod tests;

/// The latest files of the local `asset/vanilla-cache`, for tests that need real vanilla files.
#[cfg(test)]
pub(crate) mod test_support {
  use std::path::{Path, PathBuf};

  /// The latest files with `extension`, sorted by file name.
  pub(crate) fn asset_latest_files(extension: &str) -> Vec<PathBuf> {
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache");
    let mut files = super::latest_files(&cache)
      .unwrap()
      .into_iter()
      .filter(|file| {
        Path::new(&file.name).extension().is_some_and(|value| value.eq_ignore_ascii_case(extension))
      })
      .map(|file| (file.name, file.path))
      .collect::<Vec<_>>();
    files.sort();
    files.into_iter().map(|(_, path)| path).collect()
  }

  /// Writes the metadata of a vanilla cache at `cache_dir` whose latest `files`, each a file name
  /// and its path in the archive, are of stage `version` of the archive `vanilla_dir`, as the
  /// metadata records it.
  pub(crate) fn write_cache_info(
    cache_dir: &Path,
    vanilla_dir: &str,
    version: &str,
    files: &[(&str, &str)],
  ) {
    let manifest = super::DerivedManifest {
      format_version: super::CACHE_SCHEMA,
      vanilla_dir: vanilla_dir.into(),
      vanilla_manifest_sha256: String::new(),
      latest_version: version.into(),
      vanilla_input_timestamps: Default::default(),
      files: files
        .iter()
        .map(|(name, object)| {
          let file = super::LatestFile {
            version: version.into(),
            sha256: String::new(),
            object: (*object).into(),
            source: String::new(),
          };
          ((*name).to_owned(), file)
        })
        .collect(),
    };
    crate::platform::fs::create_dir_all(cache_dir).unwrap();
    super::write_json(&super::cache_info_path(cache_dir), &manifest).unwrap();
  }

  /// The latest file named `name`.
  pub(crate) fn asset_latest_file(name: &str) -> PathBuf {
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache");
    super::latest_files(&cache)
      .unwrap()
      .into_iter()
      .find(|file| file.name.eq_ignore_ascii_case(name))
      .unwrap_or_else(|| panic!("No latest vanilla file named {name}"))
      .path
  }
}
