//! Cache metadata and cumulative artifact lookup.

use crate::platform::fs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::{ArchiveContent, Result};
use crate::core::config::gamebuild::GameBuild;

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
  /// The versions of the title update's patch of the DLC pack of this stage
  /// (`update/update.rpf/dlc_patch/{pack}/`), each as some game builds ship it.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub patches: Vec<PatchVariant>,
}

/// One version of the title update's patch of a DLC pack, applied over the pack's stage by the
/// game builds that ship it.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct PatchVariant {
  /// `{stage}/update_rpf/{name}`, the directory holding its files, `{name}` being the oldest of
  /// [`Self::builds`], or `installed` when only the installed game ships it.
  pub id: String,
  /// The game builds whose title update ships it, among
  /// [`VanillaCacheManifest::title_updates`].
  pub builds: Vec<u32>,
  /// Whether the installed game's title update ships it.
  pub installed: bool,
  /// The patch's directory in the title update.
  pub archives: Vec<String>,
  /// Its files of the selected extensions, relative to the installed game's files before it.
  pub changes: BTreeMap<String, FileChange>,
  /// Optional per-file classification for formats whose entries share an extension.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub file_kinds: BTreeMap<String, String>,
  /// Number of its files whose content matched the installed game's files before it.
  #[serde(default)]
  pub unchanged: usize,
  /// What the pack declares with this patch: its `setup2.xml`, `content.xml` and island hopper
  /// files taken from the patch where it has them.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub dlc: Option<super::dlc_content::DlcContent>,
}

/// Ordered manifest of changed raw files from a particular installed game's archives.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct VanillaCacheManifest {
  /// Cache schema version, currently 4.
  pub format_version: u32,
  /// The content sets extracted into the archive.
  pub contents: BTreeSet<ArchiveContent>,
  /// Installed game directory from which this cache was generated.
  pub game_dir: PathBuf,
  /// The game builds the title update's patches of the DLC packs were read for, each with the
  /// title update read for it: FiveM's copy of the build's `update/update.rpf`, of the closest
  /// build below it that has one, or the installed game's. Empty when they were read only from
  /// the installed game.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub title_updates: BTreeMap<u32, String>,
  /// Base, title update, then DLC stages in dlclist.xml order, the stages of the packs holding
  /// the versions of the title update's patches of them.
  pub versions: Vec<CacheVersion>,
}

impl VanillaCacheManifest {
  /// Resolves the latest raw file per filename at a stage, retaining unchanged parent entries.
  ///
  /// ```
  /// # use mlo_merger::core::game_archive::VanillaCacheManifest;
  /// # let cache = VanillaCacheManifest {
  /// #   format_version: 4, contents: Default::default(), game_dir: ".".into(),
  /// #   title_updates: Default::default(), versions: vec![],
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
    let changes = |changes: &BTreeMap<String, FileChange>| {
      changes
        .iter()
        .filter(|(name, _)| belongs(name))
        .map(|(name, change)| (name.clone(), change.clone()))
        .collect()
    };
    let kinds = |kinds: &BTreeMap<String, String>| {
      kinds
        .iter()
        .filter(|(name, _)| belongs(name))
        .map(|(name, kind)| (name.clone(), kind.clone()))
        .collect()
    };
    Ok(Self {
      format_version: self.format_version,
      contents: BTreeSet::from([content]),
      game_dir: self.game_dir.clone(),
      title_updates: self.title_updates.clone(),
      versions: self
        .versions
        .iter()
        .map(|version| CacheVersion {
          changes: changes(&version.changes),
          file_kinds: kinds(&version.file_kinds),
          patches: version
            .patches
            .iter()
            .map(|patch| PatchVariant {
              changes: changes(&patch.changes),
              file_kinds: kinds(&patch.file_kinds),
              ..patch.clone()
            })
            .collect(),
          ..version.clone()
        })
        .collect(),
    })
  }

  /// The stages as `gamebuild` loads them, the last stage of the installed game without one:
  /// each pack's stage followed by the version of the title update's patch of it that the build
  /// ships, as a stage of its own (`{stage}/update_rpf/{name}`) carrying no patches, and with
  /// what the pack declares with that patch. Every file change records the content before it
  /// in these stages, and a file matching it is left out.
  ///
  /// A build takes the patches read for it, or for the closest build below it among
  /// [`Self::title_updates`]. The packs after its last stage, which it does not load, take the
  /// installed game's, as do all packs without a build.
  pub fn for_build(
    &self,
    gamebuild: Option<&GameBuild>,
  ) -> Result<Self> {
    check_format_version(self.format_version)?;
    let last_stage = match gamebuild {
      Some(gamebuild) if !self.versions.is_empty() => {
        let labels = self
          .versions
          .iter()
          .map(|version| version.id.split_once('-').map_or(version.id.as_str(), |(_, label)| label))
          .collect::<Vec<_>>();
        Some(gamebuild.last_stage(&labels)?)
      }
      _ => None,
    };
    let read_for = gamebuild.and_then(|gamebuild| {
      self
        .title_updates
        .range(..=gamebuild.build)
        .next_back()
        .or_else(|| self.title_updates.iter().next())
        .map(|(build, _)| *build)
    });
    let selected = self
      .versions
      .iter()
      .enumerate()
      .map(|(index, version)| match read_for {
        Some(build) if last_stage.is_none_or(|last| index <= last) => {
          version.patches.iter().find(|patch| patch.builds.contains(&build))
        }
        _ => version.patches.iter().find(|patch| patch.installed),
      })
      .collect::<Vec<_>>();
    // The changes record the content before them as the installed game loads it, which another
    // build's patches may change.
    let rebase = self.versions.iter().zip(&selected).any(|(version, patch)| {
      patch.map(|patch| &patch.id)
        != version.patches.iter().find(|patch| patch.installed).map(|patch| &patch.id)
    });
    let mut files = BTreeMap::<String, String>::new();
    let mut versions = Vec::with_capacity(self.versions.len());
    let mut push = |mut version: CacheVersion| {
      version.parent = versions.last().map(|previous: &CacheVersion| previous.id.clone());
      if rebase {
        for (name, change) in &mut version.changes {
          change.previous_sha256 =
            files.insert(name.to_ascii_lowercase(), change.file.sha256.clone());
        }
      }
      versions.push(version);
    };
    for (version, patch) in self.versions.iter().zip(selected) {
      push(CacheVersion {
        dlc: patch.map_or(version.dlc.clone(), |patch| patch.dlc.clone()),
        patches: Vec::new(),
        ..version.clone()
      });
      if let Some(patch) = patch {
        push(CacheVersion {
          id: patch.id.clone(),
          parent: None,
          archives: patch.archives.clone(),
          changes: patch.changes.clone(),
          file_kinds: patch.file_kinds.clone(),
          unchanged: patch.unchanged,
          dlc: None,
          patches: Vec::new(),
        });
      }
    }
    Ok(Self {
      format_version: self.format_version,
      contents: self.contents.clone(),
      game_dir: self.game_dir.clone(),
      title_updates: self.title_updates.clone(),
      versions,
    })
  }

  /// A digest of the stages and their files, leaving out the unchanged counts, which count
  /// every content's files; a content view's revision changes only with that content.
  fn revision(&self) -> Result<String> {
    let mut manifest = self.clone();
    for version in &mut manifest.versions {
      version.unchanged = 0;
      for patch in &mut version.patches {
        patch.unchanged = 0;
      }
    }
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&manifest)?)))
  }
}

/// The schema version archives are written with.
pub(crate) const FORMAT_VERSION: u32 = 4;

fn check_format_version(version: u32) -> Result<()> {
  match version {
    FORMAT_VERSION => Ok(()),
    1..=3 => Err(
      format!(
        "The vanilla archive has schema {version}, which keeps only the installed game's title-update patches of the DLC packs; regenerate it with --generate-vanilla"
      )
      .into(),
    ),
    _ => Err(format!("Unsupported GTA V cache schema {version}").into()),
  }
}

/// A content set and the game build viewing it, none for the installed game.
type BuildView = (ArchiveContent, Option<u32>);

/// The decoded manifest and the view of each content set it holds, with its revision.
struct LoadedManifest {
  manifest: Arc<VanillaCacheManifest>,
  views: BTreeMap<ArchiveContent, (Arc<VanillaCacheManifest>, String)>,
  /// Each content's view as a game build loads it, once taken.
  builds: Mutex<BTreeMap<BuildView, Arc<VanillaCacheManifest>>>,
}

/// Loads the whole archive manifest from the archive root.
pub(crate) fn load_manifest(cache: &Path) -> Result<Arc<VanillaCacheManifest>> {
  Ok(load(cache)?.manifest.clone())
}

/// Loads the archive as `content` sees it ([`VanillaCacheManifest::content`]) and `gamebuild`
/// loads it ([`VanillaCacheManifest::for_build`]), with the revision of the content's view,
/// which every game build shares.
///
/// The manifest is decoded once per process while it is unchanged, as every pipeline stage
/// reads it.
pub(crate) fn load_content(
  cache: &Path,
  content: ArchiveContent,
  gamebuild: Option<&GameBuild>,
) -> Result<(Arc<VanillaCacheManifest>, String)> {
  let loaded = load(cache)?;
  let (manifest, revision) = loaded.views.get(&content).ok_or_else(|| missing_content(content))?;
  let key = (content, gamebuild.map(|gamebuild| gamebuild.build));
  let mut builds = loaded.builds.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
  if let Some(view) = builds.get(&key) {
    return Ok((view.clone(), revision.clone()));
  }
  let view = Arc::new(manifest.for_build(gamebuild)?);
  builds.insert(key, view.clone());
  Ok((view, revision.clone()))
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
      builds: Mutex::default(),
    })
  })
}
