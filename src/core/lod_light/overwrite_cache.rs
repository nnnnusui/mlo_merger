use super::{
  PreparedEdits, Result,
  io::{Fingerprint, reject_symlinks},
};
use crate::core::source_cache::{load_stream_inventory, read_cache_entry, write_cache_entry};
use serde::{Deserialize, Serialize};
use std::{
  collections::BTreeMap,
  path::{Path, PathBuf},
};
use walkdir::WalkDir;

const VERSION: u32 = 3;

#[derive(Serialize, Deserialize)]
pub(crate) struct OverwriteCache {
  version: u32,
  source: PathBuf,
  overwrite: PathBuf,
  inputs: BTreeMap<PathBuf, Fingerprint>,
  inventory: BTreeMap<String, BTreeMap<String, (PathBuf, PathBuf)>>,
  prepared: PreparedEdits,
  #[serde(skip)]
  reused: bool,
}

impl PreparedEdits {
  /// Reuses validated models only when diff membership, resource topology and content agree.
  pub(crate) fn load_cached(
    source: &Path,
    cache: &Path,
    overwrite: &Path,
    force: bool,
  ) -> Result<(Self, OverwriteCache)> {
    let source = source.canonicalize()?;
    let overwrite = crate::common::path::absolute_path(overwrite)?;
    reject_symlinks(&overwrite)?;
    let previous = read_cache_entry::<OverwriteCache>(cache, "overwrite_cache_info.json")?
      .filter(|old| old.version == VERSION && old.source == source && old.overwrite == overwrite);
    let inventory = load_stream_inventory(&source, cache)?;
    let mut paths = Vec::new();
    if overwrite.exists() {
      for entry in WalkDir::new(&overwrite).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_symlink() {
          return Err("Symlink in overwrite directory".into());
        }
        if entry.file_type().is_file()
          && entry.file_name().to_string_lossy().ends_with(".diff.json")
        {
          paths.push(entry.path().to_path_buf());
        }
      }
    }
    let mut topology = BTreeMap::new();
    if !paths.is_empty() {
      for (resource, files) in &inventory {
        let mut entries = BTreeMap::new();
        for (relative, file) in files {
          paths.push(file.path.clone());
          entries.insert(relative.clone(), (file.path.clone(), file.original.clone()));
        }
        topology.insert(resource.clone(), entries);
      }
    }
    let inputs = paths
      .into_iter()
      .map(|path| {
        let fingerprint =
          Fingerprint::read(&path, previous.as_ref().and_then(|old| old.inputs.get(&path)), force)?;
        Ok((path, fingerprint))
      })
      .collect::<Result<BTreeMap<_, _>>>()?;
    let reusable = !force
      && previous.as_ref().is_some_and(|old| {
        old.inventory == topology
          && old.inputs.len() == inputs.len()
          && inputs.iter().all(|(path, fingerprint)| {
            old
              .inputs
              .get(path)
              .is_some_and(|old| old.sha256 == fingerprint.sha256 && old.size == fingerprint.size)
          })
      });
    let mut prepared = if reusable {
      log::debug!("Reusing overwrite preparation cache");
      previous.ok_or("Missing overwrite cache")?.prepared
    } else {
      PreparedEdits::load_inventory(&inventory, &overwrite)?
    };
    prepared.fingerprints = inputs.clone();
    prepared.verify()?;
    let mut stored = prepared.clone();
    stored.fingerprints.clear();
    Ok((
      prepared,
      OverwriteCache {
        version: VERSION,
        source,
        overwrite,
        inputs,
        inventory: topology,
        prepared: stored,
        reused: reusable,
      },
    ))
  }
}

impl OverwriteCache {
  #[cfg(test)]
  pub(crate) fn reused(&self) -> bool {
    self.reused
  }

  /// Commits freshness only after merge publication succeeds; unchanged metadata keeps its mtime.
  pub(crate) fn save(
    &self,
    cache: &Path,
  ) -> Result<()> {
    log::debug!("Overwrite preparation cache reused: {}", self.reused);
    if read_cache_entry::<Self>(cache, "overwrite_cache_info.json")?.is_some_and(|old| {
      old.version == self.version
        && old.source == self.source
        && old.overwrite == self.overwrite
        && old.inputs == self.inputs
        && old.inventory == self.inventory
    }) {
      return Ok(());
    }
    write_cache_entry(cache, "overwrite_cache_info.json", self)
  }
}
