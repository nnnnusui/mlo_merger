//! Cache metadata and cumulative artifact lookup.

use crate::platform::fs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{ArchiveContent, Result};

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

impl CachedFile {
  /// Where the file is in the vanilla archive at `archive`; see [`object_path`].
  pub fn path(
    &self,
    archive: &Path,
  ) -> Result<PathBuf> {
    object_path(archive, &self.object)
  }
}

/// Where the file the manifest records at `object` is in the vanilla archive at `archive`,
/// rejecting a path that leaves the archive.
pub fn object_path(
  archive: &Path,
  object: &str,
) -> Result<PathBuf> {
  let relative = Path::new(object);
  if object.is_empty()
    || object.contains(['\\', ':'])
    || relative.components().any(|component| !matches!(component, std::path::Component::Normal(_)))
  {
    return Err(format!("Invalid vanilla artifact path: {object}").into());
  }
  Ok(archive.join(relative))
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
  /// Lowercase filenames of the selected extensions added or replaced at this stage.
  pub changes: BTreeMap<String, FileChange>,
  /// Optional per-file classification for formats whose entries share an extension.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub file_kinds: BTreeMap<String, String>,
  /// Number of filenames whose content matched the preceding cumulative stage.
  #[serde(default)]
  pub unchanged: usize,
  /// What the DLC pack of this stage declares about the archives it mounts.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub dlc: Option<super::dlc_content::DlcContent>,
}

/// Ordered manifest of changed raw files from a particular installed game's archives.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct VanillaCacheManifest {
  /// Cache schema version, currently 2.
  pub format_version: u32,
  /// The content sets extracted into the archive; schema 1 archives hold maps only.
  #[serde(default = "map_content")]
  pub contents: BTreeSet<ArchiveContent>,
  /// Installed game directory from which this cache was generated.
  pub game_dir: PathBuf,
  /// Base, title update, then DLC stages in dlclist.xml order.
  pub versions: Vec<CacheVersion>,
}

impl VanillaCacheManifest {
  /// Resolves the latest raw file per filename at a stage, retaining unchanged parent entries.
  ///
  /// ```
  /// # use mlo_merger::core::game_archive::VanillaCacheManifest;
  /// # let cache = VanillaCacheManifest {
  /// #   format_version: 2, contents: Default::default(), game_dir: ".".into(), versions: vec![],
  /// # };
  /// assert!(cache.resolve_version("unknown").is_err());
  /// ```
  pub fn resolve_version(
    &self,
    id: &str,
  ) -> Result<BTreeMap<String, CachedFile>> {
    check_format_version(self.format_version)?;
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

  /// The archive as `content` sees it: every stage, with only that content's files.
  ///
  /// Fails when the archive holds no `content`, naming how to extract it.
  pub fn content(
    &self,
    content: ArchiveContent,
  ) -> Result<Self> {
    if !self.contents.contains(&content) {
      return Err(missing_content(content));
    }
    let belongs = |name: &String| ArchiveContent::of_file(name) == Some(content);
    Ok(Self {
      format_version: self.format_version,
      contents: BTreeSet::from([content]),
      game_dir: self.game_dir.clone(),
      versions: self
        .versions
        .iter()
        .map(|version| CacheVersion {
          changes: version
            .changes
            .iter()
            .filter(|(name, _)| belongs(name))
            .map(|(name, change)| (name.clone(), change.clone()))
            .collect(),
          file_kinds: version
            .file_kinds
            .iter()
            .filter(|(name, _)| belongs(name))
            .map(|(name, kind)| (name.clone(), kind.clone()))
            .collect(),
          ..version.clone()
        })
        .collect(),
    })
  }

  /// A digest of the stages and their files, leaving out the unchanged counts, which count
  /// every content's files; a content view's revision changes only with that content.
  fn revision(&self) -> Result<String> {
    let mut manifest = self.clone();
    for version in &mut manifest.versions {
      version.unchanged = 0;
    }
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&manifest)?)))
  }
}

/// The schema version archives are written with.
pub(crate) const FORMAT_VERSION: u32 = 2;

fn check_format_version(version: u32) -> Result<()> {
  if version == 1 || version == FORMAT_VERSION {
    Ok(())
  } else {
    Err(format!("Unsupported GTA V cache schema {version}").into())
  }
}

fn map_content() -> BTreeSet<ArchiveContent> {
  BTreeSet::from([ArchiveContent::Map])
}

/// The decoded manifest and the view of each content set it holds, with its revision.
struct LoadedManifest {
  manifest: Arc<VanillaCacheManifest>,
  views: BTreeMap<ArchiveContent, (Arc<VanillaCacheManifest>, String)>,
}

/// Loads the whole archive manifest from the archive root.
pub(crate) fn load_manifest(cache: &Path) -> Result<Arc<VanillaCacheManifest>> {
  Ok(load(cache)?.manifest.clone())
}

/// Loads the archive as `content` sees it ([`VanillaCacheManifest::content`]) with its revision.
///
/// The manifest is decoded once per process while it is unchanged, as every pipeline stage
/// reads it.
pub(crate) fn load_content(
  cache: &Path,
  content: ArchiveContent,
) -> Result<(Arc<VanillaCacheManifest>, String)> {
  let loaded = load(cache)?;
  let (manifest, revision) = loaded.views.get(&content).ok_or_else(|| missing_content(content))?;
  Ok((manifest.clone(), revision.clone()))
}

/// The error for an archive without `content`, naming how to extract it.
pub(crate) fn missing_content(content: ArchiveContent) -> Box<dyn std::error::Error> {
  format!(
    "The vanilla archive holds no {content} files; regenerate it with --generate-vanilla --content {content} (or all)"
  )
  .into()
}

/// The vanilla archive's manifest, `cache_info.json`.
pub fn manifest_path(cache: &Path) -> PathBuf {
  cache.join("cache_info.json")
}

fn load(cache: &Path) -> Result<Arc<LoadedManifest>> {
  let root = manifest_path(cache);
  if !fs::is_file(&root) {
    return Err(format!("No vanilla cache manifest found in {}", cache.display()).into());
  }
  crate::common::json::read_shared(&root, |bytes| {
    let manifest: VanillaCacheManifest = serde_json::from_slice(bytes)?;
    check_format_version(manifest.format_version)?;
    let views = manifest
      .contents
      .iter()
      .map(|&content| {
        let view = manifest.content(content)?;
        let revision = view.revision()?;
        Ok((content, (Arc::new(view), revision)))
      })
      .collect::<Result<_>>()?;
    Ok(LoadedManifest {
      manifest: Arc::new(manifest),
      views,
    })
  })
}
