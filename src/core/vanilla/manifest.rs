//! Cache metadata and cumulative artifact lookup.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::Result;

/// A cache artifact and its source within the game's archive hierarchy.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct CachedFile {
  /// SHA-256 of the extracted, standalone native file.
  pub sha256: String,
  /// Cache-relative path to the raw native file stored for this stage.
  pub object: String,
  /// Game-relative archive and entry path, including nested RPFs.
  pub source: String,
}

/// A raw file addition or replacement relative to the preceding stage.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct FileChange {
  /// Previous content hash, or None for an added file.
  pub previous_sha256: Option<String>,
  /// New raw-file artifact and provenance for this stage.
  pub file: CachedFile,
}

/// One cumulative vanilla stage, identified by dlclist order rather than build number.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
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

/// Ordered manifest of changed raw files from a particular installed game's archives.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct VanillaCacheManifest {
  /// Cache schema version, currently 1.
  pub format_version: u32,
  /// Installed game directory from which this cache was generated.
  pub game_dir: PathBuf,
  /// Base, title update, then DLC stages in dlclist.xml order.
  pub versions: Vec<CacheVersion>,
}

impl VanillaCacheManifest {
  /// Resolves the latest raw file per filename at a stage, retaining unchanged parent entries.
  ///
  /// ```
  /// # use mlo_merger::core::vanilla::VanillaCacheManifest;
  /// # let cache = VanillaCacheManifest { format_version: 1, game_dir: ".".into(), versions: vec![] };
  /// assert!(cache.resolve_version("unknown").is_err());
  /// ```
  pub fn resolve_version(
    &self,
    id: &str,
  ) -> Result<BTreeMap<String, CachedFile>> {
    if self.format_version != 1 {
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

/// Loads the schema-1 cache manifest from the cache root.
pub(crate) fn load_manifest(cache: &Path) -> Result<VanillaCacheManifest> {
  load_manifest_with_revision(cache).map(|(manifest, _)| manifest)
}

/// Reads and validates the raw manifest and hashes the same bytes that were decoded.
pub(crate) fn load_manifest_with_revision(cache: &Path) -> Result<(VanillaCacheManifest, String)> {
  let root = cache.join("cache_info.json");
  if !root.is_file() {
    return Err(format!("No vanilla cache manifest found in {}", cache.display()).into());
  }
  let bytes = fs::read(root)?;
  let manifest: VanillaCacheManifest = serde_json::from_slice(&bytes)?;
  if manifest.format_version != 1 {
    return Err(format!("Unsupported GTA V cache schema {}", manifest.format_version).into());
  }
  Ok((manifest, hex::encode(Sha256::digest(&bytes))))
}
