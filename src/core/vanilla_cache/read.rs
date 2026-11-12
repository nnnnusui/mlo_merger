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
  let loaded = read_derived(cache_dir)?;
  let metadata = &loaded.manifest;
  if metadata.format_version != CACHE_SCHEMA {
    return Err(format!("Unsupported vanilla-cache schema {}", metadata.format_version).into());
  }
  let archive = archive_dir_of(cache_dir, metadata);
  metadata
    .files
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
