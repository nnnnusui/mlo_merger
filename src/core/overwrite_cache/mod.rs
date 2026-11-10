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
//!
//! Every resource's version of a YMAP that resources add is merged, each compared with the
//! empty YMAP, and the additions an earlier one makes already are left out (see
//! [`crate::core::format::ymap::compare::added`]). They are merged from the resource whose name
//! sorts last, as the one loaded last, so its version wins where they conflict. A stream file
//! whose resource overwrite sets `ignore` is left out.

use crate::common::json::{read_json, write_json};
use crate::common::path::stream_key;
use crate::core::config::matching;
use crate::core::format::ymap::compare::{YmapComparisonDiff, YmapDiffFields};
use crate::core::overwrite::{OverwriteFingerprint, Overwrites};
use crate::core::source_cache::{MergeInputs, MergeSourceFile};
use crate::platform::fs;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

mod source_updates;
mod warnings;

pub use source_updates::{SOURCE_UPDATES, SourceUpdate, SourceUpdateOutcome, SourceVersion};
pub use warnings::{OverwriteWarnings, collect as warnings};

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

/// How one YMAP that resources add is merged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct AddedYmap {
  /// The stream files merged, in merge order: the one loaded last first.
  pub sources: Vec<AddedYmapSource>,
  /// The names of every resource shipping the YMAP, ignored ones included, in load order. When
  /// there are several, the merged YMAP is always written, so that it replaces theirs.
  pub shipped_by: Vec<String>,
  /// The additions of a source that an earlier source in merge order makes already, which the
  /// merge leaves out.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub left_out: Vec<LeftOutAddition>,
}

/// An addition the merge leaves out of a YMAP that resources add, as an earlier source in merge
/// order makes it already.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct LeftOutAddition {
  /// The item left out.
  #[serde(flatten)]
  pub item: AddedItem,
  /// The item of an earlier source it is the same as, which is kept.
  pub kept: AddedItem,
}

/// An item of one stream file of a YMAP that resources add.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct AddedItem {
  /// The resource's ID, its path relative to the source root.
  pub resource: String,
  /// The path below the resource's `stream` directory.
  pub stream_path: String,
  /// The array holding it, as diff documents name it, such as `entities` or
  /// `instanced_data.grass_instance_list`.
  pub field: String,
  /// Its index in that array of the stream file.
  pub index: usize,
}

/// One stream file of a YMAP that resources add.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct AddedYmapSource {
  /// The resource's ID, its path relative to the source root.
  pub resource: String,
  /// The path below the resource's `stream` directory.
  pub stream_path: String,
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
    let mut files = resolve(&inputs, &overwrites);
    record_left_out(&mut files, &inputs, &overwrites)?;
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

/// The stream files each added YMAP of `inputs` is merged from, leaving out those `overwrites`
/// ignore, as the overwrite cache records them; for a merge without one.
pub fn resolve_without_cache(
  inputs: &MergeInputs,
  overwrites: &Overwrites,
) -> BTreeMap<String, AddedYmap> {
  resolve(inputs, overwrites)
}

/// The stream files of `inputs` each added YMAP is merged from; see the module docs.
fn resolve(
  inputs: &MergeInputs,
  overwrites: &Overwrites,
) -> BTreeMap<String, AddedYmap> {
  let mut shippers = BTreeMap::<String, Vec<&MergeSourceFile>>::new();
  for source in &inputs.ymap {
    let name = source.file_name.to_ascii_lowercase();
    if inputs.added_ymaps.contains(&name) {
      shippers.entry(name).or_default().push(source);
    }
  }
  let mut files = BTreeMap::new();
  for (name, mut sources) in shippers {
    // Resource names are unique, so the last by lowercase name is the last loaded.
    sources.sort_by_key(|source| source.resource_name().to_ascii_lowercase());
    let shipped_by = sources.iter().map(|source| source.resource_name()).collect();
    let merged = sources
      .iter()
      .rev()
      .filter(|source| {
        let ignored = overwrites.for_source(source).is_some_and(|overwrite| overwrite.ignore);
        if ignored {
          tracing::info!(
            "Added YMAP {name} of {} is ignored by its overwrite",
            source.resource_name()
          );
        }
        !ignored
      })
      .map(|source| AddedYmapSource {
        resource: source.resource.clone(),
        stream_path: source.stream_path.replace('\\', "/"),
      })
      .collect::<Vec<_>>();
    if merged.is_empty() {
      continue;
    }
    files.insert(
      name,
      AddedYmap {
        sources: merged,
        shipped_by,
        left_out: Vec::new(),
      },
    );
  }
  files
}

/// Records in each of `files` merged from several sources the additions the merge leaves out,
/// thinning their source-cache diff documents, with their overwrites applied, as it does (see
/// [`crate::core::format::ymap::compare::added`]).
fn record_left_out(
  files: &mut BTreeMap<String, AddedYmap>,
  inputs: &MergeInputs,
  overwrites: &Overwrites,
) -> Result<()> {
  let by_key = inputs.ymap_by_key();
  let tolerances = matching::current();
  for (name, file) in files.iter_mut().filter(|(_, file)| file.sources.len() > 1) {
    let mut full = Vec::new();
    let mut kept = Vec::new();
    for entry in &file.sources {
      let source = by_key
        .get(&(
          crate::core::source_cache::resource_name(&entry.resource).to_ascii_lowercase(),
          stream_key(&entry.stream_path),
        ))
        .ok_or_else(|| format!("Source {} of added YMAP {name} is not cached", entry.resource))?;
      let path = source.diff_path.as_ref().ok_or_else(|| {
        format!("Source {} of added YMAP {name} has no diff document", entry.resource)
      })?;
      let document: YmapComparisonDiff = read_json(path)?;
      let mut diffs = document.diffs.clone();
      if let Some(overwrite) = overwrites.for_source(source) {
        overwrite.apply(
          &mut diffs,
          &OverwriteFingerprint::from(&document.source),
          &OverwriteFingerprint::from(&document.vanilla),
          name,
        )?;
      }
      full.push(serde_json::to_value(&document.diffs)?);
      kept.push(serde_json::to_value(&diffs)?);
    }
    let mut thinned = kept
      .iter()
      .map(|diffs| serde_json::from_value::<YmapDiffFields>(diffs.clone()))
      .collect::<std::result::Result<Vec<_>, _>>()?;
    let left_out =
      crate::core::format::ymap::compare::added::thin_additions(&mut thinned, &tolerances).left_out;
    // An addition's place among the changes an overwrite leaves, to its index in the source:
    // a diff against the empty YMAP adds every item, in order.
    let item = |(source, position): (usize, usize), field: &str| -> Result<AddedItem> {
      let index = source_index(&full[source], &kept[source], field, position)
        .ok_or_else(|| format!("Cannot place addition {position} of {field} in {name}"))?;
      let entry = &file.sources[source];
      Ok(AddedItem {
        resource: entry.resource.clone(),
        stream_path: entry.stream_path.clone(),
        field: field.to_string(),
        index,
      })
    };
    file.left_out = left_out
      .iter()
      .map(|left_out| {
        Ok(LeftOutAddition {
          item: item(left_out.at, left_out.field)?,
          kept: item(left_out.kept, left_out.field)?,
        })
      })
      .collect::<Result<_>>()?;
  }
  Ok(())
}

/// The index in `full`'s list of changes to `field` of the change at `position` of `kept`'s,
/// which holds some of those changes in the same order.
fn source_index(
  full: &Value,
  kept: &Value,
  field: &str,
  position: usize,
) -> Option<usize> {
  let list = |diffs: &Value| {
    field.split('.').try_fold(diffs, |value, key| value.get(key)).and_then(Value::as_array).cloned()
  };
  let (full, kept) = (list(full)?, list(kept)?);
  let mut index = 0;
  for (at, change) in kept.iter().enumerate() {
    while full.get(index)? != change {
      index += 1;
    }
    if at == position {
      return Some(index);
    }
    index += 1;
  }
  None
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn additions_an_overwrite_leaves_are_placed_by_their_index_in_the_source() {
    let added = |guid: u32| serde_json::json!({"Added": {"entity": {"guid": guid}}});
    let full = serde_json::json!({"instanced_data": {"grass_instance_list": [
      added(1), added(2), added(3), added(2)
    ]}});
    // The overwrite left out the first two.
    let kept = serde_json::json!({"instanced_data": {"grass_instance_list": [added(3), added(2)]}});
    let field = "instanced_data.grass_instance_list";
    assert_eq!(source_index(&full, &kept, field, 0), Some(2));
    assert_eq!(source_index(&full, &kept, field, 1), Some(3));
    assert_eq!(source_index(&full, &kept, field, 2), None);
    assert_eq!(source_index(&full, &kept, "entities", 0), None);
  }
}
