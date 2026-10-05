//! Loads GTAV manifests and validates ordered vanilla content histories.

use super::{Result, types::Variant};
use crate::core::gtav_cache::{CacheVersion, GtavCacheManifest};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::BufReader;
use std::path::Path;

pub(super) fn history(manifest: &GtavCacheManifest) -> Result<BTreeMap<String, Vec<Variant>>> {
  if !matches!(manifest.format_version, 1..=3) || manifest.versions.is_empty() {
    return Err("A nonempty GTAV cache with schema 1, 2 or 3 is required".into());
  }
  let mut files: BTreeMap<String, Vec<Variant>> = BTreeMap::new();
  let mut ids = BTreeSet::new();
  for (index, version) in manifest.versions.iter().enumerate() {
    let expected_parent = index.checked_sub(1).map(|index| &manifest.versions[index].id);
    if version.parent.as_ref() != expected_parent || !ids.insert(&version.id) {
      return Err(format!("Invalid GTAV version chain at {}", version.id).into());
    }
    for (name, change) in &version.changes {
      if name.contains(['/', '\\']) {
        return Err(format!("Invalid vanilla filename: {name}").into());
      }
      let variants = files.entry(name.to_ascii_lowercase()).or_default();
      if variants.last().map(|variant| &variant.file.sha256) != change.previous_sha256.as_ref() {
        return Err(format!("Invalid vanilla content chain for {name} at {}", version.id).into());
      }
      variants.push(Variant {
        index,
        version: version.id.clone(),
        file: change.file.clone(),
      });
    }
  }
  Ok(files)
}

pub(super) fn load_manifest(cache: &Path) -> Result<GtavCacheManifest> {
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
