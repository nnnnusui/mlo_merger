//! Validated derived-cache metadata for downstream processing stages.

use super::{CACHE_SCHEMA, DerivedManifest, Result, YmapRelationshipIndex};
use crate::platform::fs;
use rayon::prelude::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
  collections::BTreeMap,
  path::{Component, Path, PathBuf},
  sync::Arc,
};

#[derive(Debug, Deserialize)]
pub(crate) struct DerivedVanillaFile {
  pub(crate) version: String,
  pub(crate) sha256: String,
}

/// The derived cache's metadata and the SHA-256 of its bytes.
pub(super) struct LoadedDerived {
  pub(super) manifest: DerivedManifest,
  pub(super) sha256: String,
}

/// Reads the derived cache's metadata, decoded once per process while it is unchanged, as both
/// the vanilla-cache step and the steps after it read it.
pub(super) fn read_derived(cache_dir: &Path) -> Result<Arc<LoadedDerived>> {
  let path = cache_info_path(cache_dir);
  crate::common::json::read_shared(&path, |bytes| {
    Ok(LoadedDerived {
      manifest: serde_json::from_slice(bytes)?,
      sha256: hex::encode(Sha256::digest(bytes)),
    })
  })
  .map_err(|error| format!("Cannot read {}: {error}", path.display()).into())
}

/// The derived cache's metadata, written whenever the cache is rebuilt, so it changes with every
/// update of the latest files.
pub fn cache_info_path(cache_dir: &Path) -> PathBuf {
  cache_dir.join("cache_info.json")
}

/// The vanilla version whose files the cache in `cache_dir` names as the latest.
pub fn latest_version(cache_dir: &Path) -> Result<String> {
  Ok(read_derived(cache_dir)?.manifest.latest_version.clone())
}

/// The child YMAP names of each latest vanilla YMAP, keyed by the parent's file name
/// (`hash_XXXXXXXX` when unresolved).
pub fn ymap_children_by_parent(cache_dir: &Path) -> Result<BTreeMap<String, Vec<String>>> {
  let index: YmapRelationshipIndex =
    crate::common::json::read_json(cache_dir.join("ymap_relationships.json"))?;
  Ok(index.children_by_parent)
}

/// The vanilla archive the cache in `cache_dir` names its latest files in.
pub(crate) fn archive_dir(cache_dir: &Path) -> Result<PathBuf> {
  Ok(archive_dir_of(cache_dir, &read_derived(cache_dir)?.manifest))
}

/// `cache_dir` joined with the recorded archive, its `..` resolved without following links, so
/// that paths in the archive keep `cache_dir`'s spelling.
fn archive_dir_of(
  cache_dir: &Path,
  manifest: &DerivedManifest,
) -> PathBuf {
  let mut archive = PathBuf::new();
  for component in cache_dir.join(&manifest.vanilla_dir).components() {
    match component {
      Component::CurDir => {}
      Component::ParentDir
        if matches!(archive.components().next_back(), Some(Component::Normal(_))) =>
      {
        archive.pop();
      }
      component => archive.push(component),
    }
  }
  archive
}

/// One latest vanilla stream file of the derived cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VanillaLatestFile {
  /// Stream file name, such as `hei_ch1_11.ymap`.
  pub name: String,
  /// The vanilla version the file comes from.
  pub version: String,
  /// The file's path in the vanilla archive, with `/` separators, such as
  /// `0000-base/ymap/hei_ch1_11.ymap`.
  pub object: String,
  /// The file, in the vanilla archive.
  pub path: PathBuf,
}

/// Lists the latest vanilla stream files recorded in the derived cache's metadata, as files of
/// the vanilla archive it names, without checking them against the archive.
pub fn latest_files(cache_dir: &Path) -> Result<Vec<VanillaLatestFile>> {
  recorded_files(cache_dir, |manifest| &manifest.files)
}

/// Lists the YMAPs first shipped after the cache's stage that an older generation the game loads
/// stands for, as [`latest_files`] lists the latest files.
pub fn newer_files(cache_dir: &Path) -> Result<Vec<VanillaLatestFile>> {
  recorded_files(cache_dir, |manifest| &manifest.newer_files)
}

/// The vanilla YMAPs source YMAPs are diffed against: the latest ones and [`newer_files`].
pub(crate) fn ymap_bases(cache_dir: &Path) -> Result<Vec<VanillaLatestFile>> {
  bases(cache_dir, ".ymap")
}

/// The vanilla YBNs source YBNs are diffed against: the latest ones and [`newer_files`].
pub(crate) fn ybn_bases(cache_dir: &Path) -> Result<Vec<VanillaLatestFile>> {
  bases(cache_dir, ".ybn")
}

fn bases(
  cache_dir: &Path,
  extension: &str,
) -> Result<Vec<VanillaLatestFile>> {
  let mut files = latest_files(cache_dir)?;
  files.extend(newer_files(cache_dir)?);
  files.retain(|file| file.name.to_ascii_lowercase().ends_with(extension));
  Ok(files)
}

fn recorded_files(
  cache_dir: &Path,
  files: impl Fn(&DerivedManifest) -> &BTreeMap<String, super::LatestFile>,
) -> Result<Vec<VanillaLatestFile>> {
  let loaded = read_derived(cache_dir)?;
  let metadata = &loaded.manifest;
  if metadata.format_version != CACHE_SCHEMA {
    return Err(format!("Unsupported vanilla-cache schema {}", metadata.format_version).into());
  }
  let archive = archive_dir_of(cache_dir, metadata);
  files(metadata)
    .iter()
    .map(|(name, file)| {
      Ok(VanillaLatestFile {
        name: name.clone(),
        version: file.version.clone(),
        object: file.object.clone(),
        path: super::checked_source_path(&archive, &file.object)?,
      })
    })
    .collect()
}

/// Validated metadata and a revision covering the exact manifest/index bytes read.
pub(crate) struct LoadedCache {
  pub(crate) latest_version: String,
  /// The latest files and the newer YMAPs an older generation stands for.
  pub(crate) files: BTreeMap<String, DerivedVanillaFile>,
  pub(crate) relationships: YmapRelationshipIndex,
  pub(crate) revision: String,
}

/// Loads each metadata file once, validating its schema, provenance and artifacts.
///
/// The revision combines the SHA-256 of both files' bytes.
pub(crate) fn load_cache(
  directory: &Path,
  raw_revision: &str,
) -> Result<LoadedCache> {
  let derived = read_derived(directory)?;
  let metadata = &derived.manifest;
  let relationship_bytes = fs::read(directory.join("ymap_relationships.json"))?;
  if metadata.format_version != CACHE_SCHEMA || metadata.vanilla_manifest_sha256 != raw_revision {
    return Err("Derived vanilla cache is stale or has an unsupported schema".into());
  }
  let relationships: YmapRelationshipIndex = serde_json::from_slice(&relationship_bytes)?;
  if relationships.format_version != CACHE_SCHEMA
    || relationships.vanilla_manifest_sha256 != raw_revision
    || relationships.version != metadata.latest_version
  {
    return Err("Derived vanilla YMAP relationship index is stale".into());
  }
  // One stat per latest file, tens of thousands in all, so they run in parallel.
  let archive = archive_dir_of(directory, metadata);
  let paths = metadata
    .files
    .values()
    .chain(metadata.newer_files.values())
    .map(|file| super::checked_source_path(&archive, &file.object))
    .collect::<Result<Vec<_>>>()?;
  if let Some(missing) = paths.par_iter().find_any(|path| !fs::is_file(path)) {
    return Err(format!("Latest vanilla file is missing: {}", missing.display()).into());
  }
  let mut digest = Sha256::new();
  digest.update(b"cache_info.json");
  digest.update(derived.sha256.as_bytes());
  digest.update(b"ymap_relationships.json");
  digest.update(Sha256::digest(&relationship_bytes));
  Ok(LoadedCache {
    latest_version: metadata.latest_version.clone(),
    files: metadata
      .files
      .iter()
      .chain(&metadata.newer_files)
      .map(|(name, file)| {
        let file = DerivedVanillaFile {
          version: file.version.clone(),
          sha256: file.sha256.clone(),
        };
        (name.clone(), file)
      })
      .collect(),
    relationships,
    revision: hex::encode(digest.finalize()),
  })
}

/// One version of a stream file the vanilla archive stores.
#[derive(Debug, Clone)]
pub(crate) struct ArchivedVersion {
  pub(crate) path: PathBuf,
  /// Its path in the vanilla archive.
  pub(crate) object: String,
  pub(crate) sha256: String,
}

/// Every distinct version of each YMAP and YBN the vanilla archive of the cache at `cache_dir`
/// stores, those of every stage and of every game build's title-update patches, keyed by
/// lowercase file name, oldest first; empty when the archive cannot be read.
pub(crate) fn archived_versions(
  cache_dir: &Path
) -> std::collections::HashMap<String, Vec<ArchivedVersion>> {
  let read = || -> Result<_> {
    let archive = archive_dir(cache_dir)?;
    Ok((crate::core::game_archive::load_manifest(&archive)?, archive))
  };
  let (manifest, archive) = match read() {
    Ok(read) => read,
    Err(error) => {
      tracing::debug!("No vanilla versions to find source baselines among: {error}");
      return Default::default();
    }
  };
  let mut versions = std::collections::HashMap::<String, Vec<ArchivedVersion>>::new();
  let changes = manifest.versions.iter().flat_map(|version| {
    version.changes.iter().chain(version.patches.iter().flat_map(|patch| patch.changes.iter()))
  });
  for (name, change) in changes {
    let name = name.to_ascii_lowercase();
    if !name.ends_with(".ymap") && !name.ends_with(".ybn") {
      continue;
    }
    let known = versions.entry(name).or_default();
    if known.iter().any(|version| version.sha256 == change.file.sha256) {
      continue;
    }
    let Ok(path) = change.file.path(&archive) else {
      continue;
    };
    known.push(ArchivedVersion {
      path,
      object: change.file.object.clone(),
      sha256: change.file.sha256.clone(),
    });
  }
  versions
}

/// A digest of `versions`, recorded by the diffs that found a baseline among them; `None` for a
/// single version, which is the latest.
pub(crate) fn versions_digest(versions: &[ArchivedVersion]) -> Option<String> {
  if versions.len() < 2 {
    return None;
  }
  let mut digest = Sha256::new();
  for version in versions {
    digest.update(version.sha256.as_bytes());
  }
  Some(hex::encode(digest.finalize()))
}
