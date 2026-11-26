//! Writes each source YMAP's differences from vanilla as `diff/{resource}/{path}.diff.json`,
//! and each source YBN's named after a vanilla YBN ([`YbnComparisonDiff`]).
//!
//! A YMAP a resource adds, named after no vanilla YMAP, is compared with an empty YMAP instead
//! (see [`crate::app::map::source_cache`]'s `added` module).
//!
//! A diff is rewritten only when the source or vanilla YMAP it was taken from changed; the
//! document records both files' fingerprints for that check. Checking reads every document, so
//! `diff/inputs.json` records what the diffs were taken under: the diff layout, the matching
//! tolerances, the vanilla cache revision and a digest of each resource's inventoried YMAPs.
//! While those match, a resource's diffs are kept without being read; only resources whose
//! inventory changed, or every resource when the rest changed, are checked file by file.

use super::Result;
use crate::platform::fs;
use crate::{
  app::map::{
    compare::{StreamFileRef, YmapComparisonContext},
    source_cache::{stream_file_diff_path, types::ResourceInventory},
  },
  common::{json::write_json, path::ensure_safe_path},
  core::config::matching::{self, MatchTolerances},
  core::{
    conversion::load_hash_names_or_empty,
    format::{
      ybn::diff::polygons::{self, YbnComparisonDiff},
      ymap::compare::{YmapComparisonDiff, YmapComparisonFile, YmapComparisonPartner},
    },
  },
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

/// File name of the record of what the diffs were taken under, in the `diff` directory.
const INPUTS_FILE: &str = "inputs.json";
/// Bump when [`DiffRecord`] or what its digests cover changes.
const RECORD_VERSION: u32 = 3;

/// What the diffs in a `diff` directory were taken under; see the module docs.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct DiffRecord {
  record_version: u32,
  prerequisites: Prerequisites,
  /// A digest of each resource's inventoried YMAPs, keyed by resource ID.
  resources: BTreeMap<String, String>,
}

/// What every diff depends on beyond its own resource.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Prerequisites {
  diff_schema_version: u32,
  ybn_diff_schema_version: u32,
  tolerances: MatchTolerances,
  vanilla_cache_revision: String,
}

/// How many diff documents [`write_ymap_diffs`] wrote and kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app::map::source_cache) struct YmapDiffCounts {
  pub written: usize,
  pub reused: usize,
}

/// The layout, tolerances and fingerprints of a diff document, read without its differences.
#[derive(serde::Deserialize)]
struct DiffInputs {
  #[serde(default)]
  schema_version: u32,
  #[serde(default)]
  tolerances: Option<MatchTolerances>,
  target: YmapComparisonFile,
  base: YmapComparisonFile,
  #[serde(default)]
  partner: Option<YmapComparisonPartner>,
}

/// Rebuilds the `diff` directory for every source YMAP that overrides a vanilla YMAP or that a
/// resource adds, which is compared with an empty YMAP.
///
/// Reads the published inventory (`cache_info.json`), so it runs after publication. Diffs whose
/// source and vanilla YMAPs are unchanged are kept; the rest are regenerated, and diffs of YMAPs
/// no longer overriding vanilla are dropped. YMAPs whose diff cannot be built are logged and
/// skipped.
pub(in crate::app::map::source_cache) fn write_ymap_diffs(
  source_dir: &Path,
  output: &Path,
  vanilla_dir: &Path,
  vanilla_cache_dir: &Path,
) -> Result<YmapDiffCounts> {
  let metadata = crate::app::map::source_cache::read_metadata(output)?;
  let staging = output.join("diff.tmp");
  let target = output.join("diff");
  ensure_safe_path(output, &staging)?;
  ensure_safe_path(output, &target)?;
  remove_dir_if_exists(&staging)?;

  let mut counts = YmapDiffCounts {
    written: 0,
    reused: 0,
  };
  // Diffs of resources the record vouches for are kept as they are. The others are checked in
  // parallel, since a check reads its whole document; current ones are then moved to staging
  // in order, and the rest are taken in parallel, each from its own source and vanilla YMAPs.
  // Worker threads do not inherit the active matching tolerances, so each is handed them.
  let tolerances = matching::current();
  let prerequisites = Prerequisites {
    diff_schema_version: YmapComparisonDiff::SCHEMA_VERSION,
    ybn_diff_schema_version: YbnComparisonDiff::SCHEMA_VERSION,
    tolerances,
    vanilla_cache_revision: metadata.vanilla_cache_revision.clone(),
  };
  let record = read_record(&target.join(INPUTS_FILE))
    .filter(|record| record.record_version == RECORD_VERSION)
    .filter(|record| record.prerequisites == prerequisites);
  let mut digests = BTreeMap::new();
  let mut entries = Vec::new();
  let mut ybn_entries = Vec::new();
  for (resource_id, resource) in &metadata.resources {
    let digest = resource_digest(resource)?;
    let vouched =
      record.as_ref().is_some_and(|record| record.resources.get(resource_id) == Some(&digest));
    digests.insert(resource_id.clone(), digest);
    let resource_name =
      resource.source.file_name().ok_or("Resource has no name")?.to_string_lossy().into_owned();
    for file in resource.files_by_format.get(".ybn").into_iter().flatten() {
      if file.vanilla.is_none() {
        continue;
      }
      let file_path = stream_file_diff_path(&resource.source, &file.path)?;
      let (previous, path) = (target.join(&file_path), staging.join(&file_path));
      ensure_safe_path(&target, &previous)?;
      ensure_safe_path(&staging, &path)?;
      let relative = resource.source.join(&file.path).to_string_lossy().replace('\\', "/");
      let (source, stored) = match &file.cached_path {
        Some(cached) => (output.join(cached), cached.clone()),
        None => (source_dir.join(&relative), relative),
      };
      ybn_entries.push(YbnEntry {
        source,
        stored,
        file_name: file.file_name.to_ascii_lowercase(),
        previous,
        path,
        vouched,
      });
    }
    for file in resource.files_by_format.get(".ymap").into_iter().flatten() {
      // A YMAP a resource adds is compared with an empty one; only moved ones are merged.
      if file.vanilla.is_none() && file.cached_path.is_none() {
        continue;
      }
      let file_path = stream_file_diff_path(&resource.source, &file.path)?;
      let stream_file = StreamFileRef {
        resource_name: Some(resource_name.clone()),
        vanilla_version: metadata.latest_vanilla_version.clone(),
        file_name: file.file_name.clone(),
      };
      let (previous, path) = (target.join(&file_path), staging.join(&file_path));
      ensure_safe_path(&target, &previous)?;
      ensure_safe_path(&staging, &path)?;
      let kept = vouched && fs::is_file(&previous);
      entries.push((stream_file, file_path, previous, path, kept));
    }
  }
  // Checks need no hash names, which only decoding YMAPs for new diffs does; loading them is
  // most of a run in which every diff is current.
  let unchecked = entries.iter().any(|(.., kept)| !kept);
  let context = unchecked
    .then(|| YmapComparisonContext::new(source_dir, output, vanilla_cache_dir, Default::default()))
    .transpose()?;
  let current = entries
    .par_iter()
    .map(|(stream_file, _, previous, _, kept)| {
      *kept
        || context.as_ref().is_some_and(|context| {
          matching::with_tolerances(tolerances, || is_current(context, stream_file, previous))
        })
    })
    .collect::<Vec<_>>();
  let (kept, stale): (Vec<_>, Vec<_>) =
    entries.into_iter().zip(current).partition(|(_, current)| *current);
  kept
    .par_iter()
    .map(|((_, _, previous, path, _), _)| {
      let parent = path.parent().ok_or("Diff path has no parent")?;
      fs::create_dir_all(parent)
        .and_then(|()| fs::rename(previous, path))
        .map_err(|error| format!("{}: {error}", path.display()))
    })
    .collect::<std::result::Result<(), String>>()?;
  counts.reused = kept.len();
  let stale = stale
    .into_iter()
    .map(|((stream_file, file_path, _, path, _), _)| (stream_file, file_path, path))
    .collect::<Vec<_>>();
  if !stale.is_empty() {
    let hash_names = load_hash_names_or_empty(&vanilla_dir.join("hash_names.json"));
    let context = YmapComparisonContext::new(source_dir, output, vanilla_cache_dir, hash_names)?;
    counts.written = take_diffs(&context, &stale, tolerances)?;
  }

  let (ybn_written, ybn_kept) = write_ybn_diffs(&ybn_entries, vanilla_cache_dir, tolerances)?;

  fs::create_dir_all(&staging)?;
  write_json(
    &staging.join(INPUTS_FILE),
    &DiffRecord {
      record_version: RECORD_VERSION,
      prerequisites,
      resources: digests,
    },
  )?;
  remove_dir_if_exists(&target)?;
  fs::rename(&staging, &target)?;
  tracing::info!(
    "Wrote {} and kept {} YMAP diffs and {ybn_written} and {ybn_kept} YBN diffs in {}",
    counts.written,
    counts.reused,
    target.display()
  );
  Ok(counts)
}

/// A source YBN named after a vanilla YBN: where it is, as stored, and where its diff was and goes.
struct YbnEntry {
  source: std::path::PathBuf,
  stored: String,
  file_name: String,
  previous: std::path::PathBuf,
  path: std::path::PathBuf,
  /// Whether the record vouches for its resource's diffs, so its diff is kept unread.
  vouched: bool,
}

/// Writes the diff of each of `entries` to its staging path: the previous one moved there while
/// it describes the files as they are now, otherwise a new one taken with `tolerances`. Returns
/// how many were written and kept; a YBN whose diff cannot be taken is logged and skipped.
fn write_ybn_diffs(
  entries: &[YbnEntry],
  vanilla_cache_dir: &Path,
  tolerances: MatchTolerances,
) -> Result<(usize, usize)> {
  if entries.is_empty() {
    return Ok((0, 0));
  }
  let vanilla = crate::app::map::vanilla_cache::latest_files(vanilla_cache_dir)?
    .into_iter()
    .map(|file| (file.name.to_ascii_lowercase(), (file.path, file.object)))
    .collect::<std::collections::HashMap<_, _>>();
  let results = entries
    .par_iter()
    .map(|entry| -> std::result::Result<Option<bool>, String> {
      let Some((vanilla_path, object)) = vanilla.get(&entry.file_name) else {
        return Ok(None);
      };
      let failed = |error: &dyn std::fmt::Display| format!("{}: {error}", entry.path.display());
      let comparison = |path: &Path, stored: &str, previous: Option<&YmapComparisonFile>| {
        let previous = previous.filter(|previous| previous.path == stored).map(|previous| {
          crate::common::fingerprint::Fingerprint {
            sha256: previous.sha256.clone(),
            size: previous.size,
            modified_seconds: previous.modified_seconds,
            modified_nanos: previous.modified_nanos,
          }
        });
        crate::common::fingerprint::Fingerprint::read(path, previous.as_ref(), false).map(
          |fingerprint| YmapComparisonFile {
            path: stored.to_owned(),
            sha256: fingerprint.sha256,
            size: fingerprint.size,
            modified_seconds: fingerprint.modified_seconds,
            modified_nanos: fingerprint.modified_nanos,
          },
        )
      };
      let previous = crate::common::json::read_json::<YbnComparisonDiff>(&entry.previous).ok();
      let current = previous.as_ref().is_some_and(|previous| {
        entry.vouched
          || (previous.schema_version == YbnComparisonDiff::SCHEMA_VERSION
            && previous.tolerance == tolerances.ybn_position
            && comparison(&entry.source, &entry.stored, Some(&previous.target))
              .is_ok_and(|target| target.same_content(&previous.target))
            && comparison(vanilla_path, object, Some(&previous.base))
              .is_ok_and(|base| base.same_content(&previous.base)))
      });
      fs::create_dir_all(entry.path.parent().ok_or("Diff path has no parent")?)
        .map_err(|error| failed(&error))?;
      if current {
        fs::rename(&entry.previous, &entry.path).map_err(|error| failed(&error))?;
        return Ok(Some(false));
      }
      let taken = (|| -> Result<YbnComparisonDiff> {
        let source = crate::core::format::ybn::read_ybn(&fs::read(&entry.source)?)?;
        let base = crate::core::format::ybn::read_ybn(&fs::read(vanilla_path)?)?;
        Ok(YbnComparisonDiff {
          schema_version: YbnComparisonDiff::SCHEMA_VERSION,
          target: comparison(&entry.source, &entry.stored, None)?,
          base: comparison(vanilla_path, object, None)?,
          tolerance: tolerances.ybn_position,
          diffs: polygons::diff(&base, &source, tolerances.ybn_position)?,
        })
      })();
      match taken {
        Ok(document) => {
          write_json(&entry.path, &document).map_err(|error| failed(&error))?;
          Ok(Some(true))
        }
        Err(error) => {
          tracing::warn!("Skipping YBN diff {}: {error}", entry.source.display());
          Ok(None)
        }
      }
    })
    .collect::<std::result::Result<Vec<_>, String>>()?;
  let written = results.iter().filter(|result| **result == Some(true)).count();
  let kept = results.iter().filter(|result| **result == Some(false)).count();
  Ok((written, kept))
}

/// Takes the diff of each of `stale` in parallel and writes it to its staging path; returns how
/// many were written. A diff that cannot be taken is logged and skipped.
fn take_diffs(
  context: &YmapComparisonContext,
  stale: &[(StreamFileRef, std::path::PathBuf, std::path::PathBuf)],
  tolerances: MatchTolerances,
) -> Result<usize> {
  let results = stale
    .par_iter()
    .map(|(stream_file, file_path, path)| {
      let diff = match matching::with_tolerances(tolerances, || context.get_diff(stream_file)) {
        Ok(diff) => diff,
        Err(error) => {
          tracing::warn!("Skipping YMAP diff {}: {error}", file_path.display());
          return Ok(false);
        }
      };
      let written = (|| {
        fs::create_dir_all(path.parent().ok_or("Diff path has no parent")?)?;
        write_json(path, &diff)
      })();
      written.map(|()| true).map_err(|error| format!("{}: {error}", path.display()))
    })
    .collect::<std::result::Result<Vec<_>, String>>()?;
  Ok(results.into_iter().filter(|written| *written).count())
}

fn read_record(path: &Path) -> Option<DiffRecord> {
  crate::common::json::read_json(path).ok()
}

/// A digest of what `resource`'s diffs are taken from in its inventory: each YMAP's and YBN's
/// path, content, cached location, vanilla match and parent. Modification times are left out, so
/// a touched but unchanged file keeps its diffs.
fn resource_digest(resource: &ResourceInventory) -> Result<String> {
  let ymaps = [".ymap", ".ybn"]
    .iter()
    .flat_map(|format| resource.files_by_format.get(*format).into_iter().flatten())
    .map(|file| {
      (
        &file.path,
        &file.sha256,
        file.size,
        &file.cached_path,
        &file.vanilla,
        &file.ymap_parent_hash,
      )
    })
    .collect::<Vec<_>>();
  Ok(hex::encode(Sha256::digest(serde_json::to_vec(&(&resource.source, ymaps))?)))
}

/// Whether the diff at `path` exists in the current layout, was taken with the active matching
/// tolerances and from the files `file` resolves to now.
///
/// Unreadable diffs and files are treated as stale, so they are regenerated.
fn is_current(
  context: &YmapComparisonContext,
  file: &StreamFileRef,
  path: &Path,
) -> bool {
  let Ok(inputs) = crate::common::json::read_json::<DiffInputs>(path) else {
    return false;
  };
  inputs.schema_version == YmapComparisonDiff::SCHEMA_VERSION
    && inputs.tolerances == Some(matching::current())
    && context
      .is_diff_current(file, &inputs.target, &inputs.base, inputs.partner.as_ref())
      .unwrap_or(false)
}

fn remove_dir_if_exists(path: &Path) -> Result<()> {
  match fs::remove_dir_all(path) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  #[ignore = "requires local asset/map/source-cache and asset/map/vanilla-cache"]
  fn writes_ymap_diffs_for_local_source_cache() {
    let asset = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset");
    let source_cache = asset.join("source-cache");
    let output = std::env::temp_dir().join(format!("ymap_diffs_{}", std::process::id()));
    let _ = fs::remove_dir_all(&output);
    for entry in crate::platform::fs::WalkDir::new(&source_cache) {
      let entry = entry.unwrap();
      let destination = output.join(entry.path().strip_prefix(&source_cache).unwrap());
      if entry.file_type().is_dir() {
        fs::create_dir_all(&destination).unwrap();
      } else {
        fs::copy(entry.path(), &destination).unwrap();
      }
    }

    let written = write_ymap_diffs(
      &asset.join("source"),
      &output,
      &asset.join("vanilla"),
      &asset.join("vanilla-cache"),
    )
    .unwrap()
    .written;
    let files = crate::platform::fs::WalkDir::new(output.join("diff"))
      .into_iter()
      .filter_map(std::result::Result::ok)
      .filter(|entry| entry.file_name().to_string_lossy().ends_with(".ymap.diff.json"))
      .count();
    println!("{written} diffs -> {}", output.join("diff").display());
    assert!(written > 0);
    assert_eq!(files, written);
    assert!(!output.join("diff.tmp").exists());

    // Nothing changed since, so every diff is kept.
    let rerun = write_ymap_diffs(
      &asset.join("source"),
      &output,
      &asset.join("vanilla"),
      &asset.join("vanilla-cache"),
    )
    .unwrap();
    assert_eq!(
      rerun,
      YmapDiffCounts {
        written: 0,
        reused: written,
      }
    );
  }
}
