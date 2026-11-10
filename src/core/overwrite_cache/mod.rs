//! What the overwrites call for, derived from them, the source cache and the vanilla cache.
//!
//! The overwrite cache is to the overwrites what the source cache is to the sources: read-side
//! preprocessing, rebuilt from them and nothing else, which the merge reads. It holds:
//!
//! - `added_ymaps.json` ([`AddedYmaps`]): for each YMAP that resources add, the stream files the
//!   merge takes it from, in merge order, and the additions it leaves out of them as an earlier
//!   one makes them already.
//! - `warnings.json` ([`OverwriteWarnings`]): overwrites that no longer match their files. The
//!   merge logs them on every run.
//! - `source_updates/` ([`SourceUpdate`]): the resource overwrites whose source changed, each as
//!   it was and what was done about it: rewritten against the current version, in the
//!   overwrites, or left to be written again. Unlike the rest, these records are kept.

use crate::common::json::{read_json, write_json};
use crate::core::config::matching;
use crate::core::overwrite::Overwrites;
use crate::platform::fs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

mod added;
mod source_updates;
mod warnings;

pub use added::{AddedItem, AddedYmap, AddedYmapSource, LeftOutAddition};
pub use source_updates::{SOURCE_UPDATES, SourceUpdate, SourceUpdateOutcome, SourceVersion};
pub use warnings::OverwriteWarnings;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// File name of the [`AddedYmaps`] record.
const ADDED_YMAPS: &str = "added_ymaps.json";
/// File name of the [`OverwriteWarnings`].
const WARNINGS: &str = "warnings.json";
/// Directories earlier formats of the cache held, removed when it is rebuilt.
const OBSOLETE_DIRS: &[&str] = &["diff", "base"];
/// Bump when what the cache holds or what [`AddedYmaps::inputs`] covers changes.
const FORMAT_VERSION: u32 = 5;

/// The stream files each YMAP that resources add is merged from (`added_ymaps.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct AddedYmaps {
  pub format_version: u32,
  /// A digest of what the cache was built from: the source and vanilla caches' inventories,
  /// every overwrite document and the matching tolerances.
  pub inputs: String,
  /// Keyed by lowercase file name.
  pub files: BTreeMap<String, AddedYmap>,
}

/// Builds or updates the overwrite cache.
#[derive(Debug, Clone)]
pub struct BuildOverwriteCache {
  pub source_dir: PathBuf,
  pub source_cache_dir: PathBuf,
  pub vanilla_cache_dir: PathBuf,
  /// The overwrites; `None` has none.
  pub overwrite_dir: Option<PathBuf>,
  pub output_dir: PathBuf,
}

impl BuildOverwriteCache {
  /// Brings the cache up to date with the overwrites and the source and vanilla caches, which
  /// must already be built; returns whether anything was written.
  pub fn run(&self) -> Result<bool> {
    matching::with_config(|| self.run_inner())?
  }

  fn run_inner(&self) -> Result<bool> {
    let source_dir = fs::canonicalize(&self.source_dir)?;
    let source_cache_dir = fs::canonicalize(&self.source_cache_dir)?;
    let vanilla_cache_dir = fs::canonicalize(&self.vanilla_cache_dir)?;
    let output = &self.output_dir;
    let mut inputs_digest =
      inputs_digest(&source_cache_dir, &vanilla_cache_dir, self.overwrite_dir.as_deref())?;
    if read_json::<AddedYmaps>(output.join(ADDED_YMAPS)).is_ok_and(|previous| {
      previous.format_version == FORMAT_VERSION && previous.inputs == inputs_digest
    }) && read_json::<OverwriteWarnings>(output.join(WARNINGS)).is_ok()
    {
      tracing::info!("Overwrite cache is current; nothing to do");
      return Ok(false);
    }
    let inputs = crate::core::source_cache::load_merge_inputs(&source_cache_dir, &source_dir)?;
    let mut overwrites = match &self.overwrite_dir {
      Some(directory) => Overwrites::load(directory)?,
      None => Overwrites::default(),
    };
    // Overwrites written against an earlier version of their source are rewritten where they
    // can be, so what follows reads them as rewritten.
    if source_updates::review(output, &source_cache_dir, &inputs, &overwrites)?
      && let Some(directory) = &self.overwrite_dir
    {
      overwrites = Overwrites::load(directory)?;
      inputs_digest = self::inputs_digest(&source_cache_dir, &vanilla_cache_dir, Some(directory))?;
    }
    let mut files = added::resolve(&inputs, &overwrites);
    added::record_left_out(&mut files, &inputs, &overwrites)?;
    let warnings = warnings::collect(&inputs, &overwrites, &vanilla_cache_dir)?;
    for directory in OBSOLETE_DIRS {
      let path = output.join(directory);
      if fs::is_dir(&path) {
        fs::remove_dir_all(&path)?;
      }
    }
    fs::create_dir_all(output)?;
    write_json(&output.join(WARNINGS), &warnings)?;
    write_json(
      &output.join(ADDED_YMAPS),
      &AddedYmaps {
        format_version: FORMAT_VERSION,
        inputs: inputs_digest,
        files,
      },
    )?;
    tracing::info!("Wrote the overwrite cache to {}", output.display());
    if !warnings.is_empty() {
      tracing::info!(
        "{} lists {} ignored overwrites",
        output.join(WARNINGS).display(),
        warnings.ignored_overwrites.len()
      );
    }
    Ok(true)
  }
}

/// The added YMAPs and warnings the overwrite cache at `cache_dir` records, which must have been
/// built from the current source and vanilla caches and overwrites.
pub fn load(
  cache_dir: &Path,
  source_cache_dir: &Path,
  vanilla_cache_dir: &Path,
  overwrite_dir: Option<&Path>,
) -> Result<(AddedYmaps, OverwriteWarnings)> {
  let path = cache_dir.join(ADDED_YMAPS);
  let added = read_json::<AddedYmaps>(&path).map_err(|error| {
    format!("Cannot read {}: {error}; build the overwrite cache", path.display())
  })?;
  let current = added.format_version == FORMAT_VERSION
    && added.inputs == inputs_digest(source_cache_dir, vanilla_cache_dir, overwrite_dir)?;
  if !current {
    return Err(
      format!(
        "{} is stale; build the overwrite cache (--generate-overwrite-cache)",
        path.display()
      )
      .into(),
    );
  }
  let path = cache_dir.join(WARNINGS);
  let warnings = read_json::<OverwriteWarnings>(&path).map_err(|error| {
    format!("Cannot read {}: {error}; build the overwrite cache", path.display())
  })?;
  Ok((added, warnings))
}

/// A digest of what the cache is built from; see [`AddedYmaps::inputs`].
fn inputs_digest(
  source_cache_dir: &Path,
  vanilla_cache_dir: &Path,
  overwrite_dir: Option<&Path>,
) -> Result<String> {
  let mut digest = Sha256::new();
  digest.update(FORMAT_VERSION.to_le_bytes());
  digest.update(serde_json::to_vec(&matching::current())?);
  for path in [
    crate::core::source_cache::cache_info_path(source_cache_dir),
    vanilla_cache_dir.join("cache_info.json"),
  ] {
    digest.update(Sha256::digest(fs::read(&path)?));
  }
  if let Some(directory) = overwrite_dir.filter(|directory| fs::is_dir(directory)) {
    let mut files = Vec::new();
    for entry in crate::platform::fs::WalkDir::new(directory).follow_links(false) {
      let entry = entry?;
      if entry.file_type().is_file() {
        files.push(entry.path().to_path_buf());
      }
    }
    files.sort();
    for path in files {
      digest.update(path.strip_prefix(directory)?.to_string_lossy().as_bytes());
      digest.update(Sha256::digest(fs::read(&path)?));
    }
  }
  Ok(hex::encode(digest.finalize()))
}
