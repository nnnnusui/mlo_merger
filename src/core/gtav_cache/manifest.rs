//! Cache metadata and cumulative artifact lookup.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use super::Result;

/// A cache artifact and its source within the game's archive hierarchy.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CachedFile {
  /// SHA-256 of the extracted, standalone native file.
  pub sha256: String,
  /// Cache-relative path to a new native file or a replacement's exact vanilla JSON delta.
  pub object: String,
  /// Optional legacy native snapshot reference; omitted by new cache generation.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub native: Option<String>,
  /// Game-relative archive and entry path, including nested RPFs.
  pub source: String,
}

/// An addition or content replacement relative to the preceding stage.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileChange {
  /// Previous content hash, or None for an added file.
  pub previous_sha256: Option<String>,
  /// New cache artifact and provenance; replacements reference .ymap.diff.json.
  pub file: CachedFile,
}

/// One cumulative vanilla stage, identified by dlclist order rather than build number.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheVersion {
  /// Stable position-prefixed ID, such as 0000-base or 0002-mpbeach.
  pub id: String,
  /// Preceding stage ID; absent for the base snapshot.
  pub parent: Option<String>,
  /// Archives contributing to this stage, in overlay order.
  pub archives: Vec<String>,
  /// Case-insensitive YMAP/YBN filenames added or replaced at this stage.
  pub changes: BTreeMap<String, FileChange>,
  /// Number of filenames whose content matched the preceding cumulative stage.
  #[serde(default)]
  pub unchanged: usize,
}

/// Ordered delta manifest for a particular installed game's archives.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GtavCacheManifest {
  /// Cache schema version, currently 3; schema 2 remains readable.
  pub format_version: u32,
  /// Installed game directory from which this cache was generated.
  pub game_dir: PathBuf,
  /// Base, title update, then DLC stages in dlclist.xml order.
  pub versions: Vec<CacheVersion>,
}

impl GtavCacheManifest {
  /// Resolves the latest artifact per filename at a stage, retaining unchanged parent entries.
  /// Replacement artifacts are diff reports, not standalone native binaries.
  ///
  /// ```
  /// # use mlo_merger::core::gtav_cache::GtavCacheManifest;
  /// # let cache = GtavCacheManifest { format_version: 2, game_dir: ".".into(), versions: vec![] };
  /// assert!(cache.resolve_version("unknown").is_err());
  /// ```
  pub fn resolve_version(
    &self,
    id: &str,
  ) -> Result<BTreeMap<String, CachedFile>> {
    if !matches!(self.format_version, 2 | 3) {
      return Err(format!("Unsupported GTA V cache schema {}", self.format_version).into());
    }
    let mut files = BTreeMap::new();
    for version in &self.versions {
      for (name, change) in &version.changes {
        files.insert(name.clone(), change.file.clone());
      }
      if version.id == id {
        return Ok(files);
      }
    }
    Err(format!("Unknown vanilla stage: {id}").into())
  }
}

/// Prefer current root metadata, then per-version records over a stale schema-1
/// manifest; the latter may still supply the original game-directory hint.
pub(crate) fn load_manifest(cache: &Path) -> Result<GtavCacheManifest> {
  let root = cache.join("cache_info.json");
  if root.is_file() {
    return Ok(serde_json::from_reader(BufReader::new(fs::File::open(root)?))?);
  }
  let legacy = cache.join("manifest.json");
  let legacy: Option<GtavCacheManifest> = if legacy.is_file() {
    Some(serde_json::from_reader(BufReader::new(fs::File::open(legacy)?))?)
  } else {
    None
  };
  if legacy.as_ref().is_some_and(|manifest| matches!(manifest.format_version, 2 | 3)) {
    return legacy.ok_or_else(|| "Missing GTAV manifest".into());
  }
  let mut versions: Vec<CacheVersion> = Vec::new();
  for entry in fs::read_dir(cache)? {
    let entry = entry?;
    if !entry.file_type()?.is_dir() {
      continue;
    }
    let info = entry.path().join("version_info.json");
    if info.is_file() {
      let version: CacheVersion = serde_json::from_reader(BufReader::new(fs::File::open(info)?))?;
      if entry.file_name().to_string_lossy() != version.id {
        return Err("Version directory does not match metadata ID".into());
      }
      versions.push(version);
    }
  }
  if !versions.is_empty() {
    versions.sort_by_key(|version| {
      version.id.split_once('-').and_then(|(index, _)| index.parse::<usize>().ok())
    });
    log::warn!("cache_info.json is missing; using per-version metadata from {}", cache.display());
    return Ok(GtavCacheManifest {
      format_version: 2,
      game_dir: legacy.map(|manifest| manifest.game_dir).unwrap_or_default(),
      versions,
    });
  }
  legacy.ok_or_else(|| format!("No GTAV cache metadata found in {}", cache.display()).into())
}
