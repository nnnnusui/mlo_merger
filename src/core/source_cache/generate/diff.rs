//! Writes each source YMAP's differences from vanilla as `diff/{resource}/{path}.diff.json`.
//!
//! A diff is rewritten only when the source or vanilla YMAP it was taken from changed; the
//! document records both files' fingerprints for that check.

use super::Result;
use crate::{
  common::{json::write_json, path::ensure_safe_path},
  core::config::matching::{self, MatchTolerances},
  core::{
    conversion::load_hash_names_or_empty,
    format::ymap::compare::{
      StreamFileRef, YmapComparisonContext, YmapComparisonDiff, YmapComparisonFile,
    },
    source_cache::{types::SourceCacheMetadata, ymap_diff_path},
  },
};
use rayon::prelude::*;
use std::{fs, io::BufReader, path::Path};

/// How many diff documents [`write_ymap_diffs`] wrote and kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::core::source_cache) struct YmapDiffCounts {
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
  source: YmapComparisonFile,
  vanilla: YmapComparisonFile,
}

/// Rebuilds the `diff` directory for every source YMAP that overrides a vanilla YMAP.
///
/// Reads the published inventory (`cache_info.json`), so it runs after publication. Diffs whose
/// source and vanilla YMAPs are unchanged are kept; the rest are regenerated, and diffs of YMAPs
/// no longer overriding vanilla are dropped. YMAPs whose diff cannot be built are logged and
/// skipped.
pub(in crate::core::source_cache) fn write_ymap_diffs(
  output: &Path,
  vanilla_dir: &Path,
  vanilla_cache_dir: &Path,
) -> Result<YmapDiffCounts> {
  let metadata: SourceCacheMetadata = serde_json::from_reader(BufReader::new(fs::File::open(
    crate::core::source_cache::cache_info_path(output),
  )?))?;
  let hash_names = load_hash_names_or_empty(&vanilla_dir.join("hash_names.json"));
  let context = YmapComparisonContext::new(output, vanilla_cache_dir, hash_names)?;

  let staging = output.join("diff.tmp");
  let target = output.join("diff");
  ensure_safe_path(output, &staging)?;
  ensure_safe_path(output, &target)?;
  remove_dir_if_exists(&staging)?;

  let mut counts = YmapDiffCounts {
    written: 0,
    reused: 0,
  };
  // Current diffs are moved to staging as they are found; the others are taken afterwards in
  // parallel, each from its own source and vanilla YMAPs.
  let mut stale = Vec::new();
  for resource in metadata.resources.values() {
    let resource_name =
      resource.source.file_name().ok_or("Resource has no name")?.to_string_lossy().into_owned();
    for file in resource.files_by_format.get(".ymap").into_iter().flatten() {
      if file.vanilla.is_none() {
        continue;
      }
      let file_path = ymap_diff_path(&resource.source, &file.path)?;
      let stream_file = StreamFileRef {
        resource_name: Some(resource_name.clone()),
        vanilla_version: metadata.latest_vanilla_version.clone(),
        file_name: file.file_name.clone(),
      };
      let (previous, path) = (target.join(&file_path), staging.join(&file_path));
      ensure_safe_path(&target, &previous)?;
      ensure_safe_path(&staging, &path)?;
      let parent = path.parent().ok_or("Diff path has no parent")?;
      if is_current(&context, &stream_file, &previous) {
        fs::create_dir_all(parent)?;
        fs::rename(&previous, &path)?;
        counts.reused += 1;
        continue;
      }
      stale.push((stream_file, file_path, path));
    }
  }
  // Worker threads do not inherit the active matching tolerances, so each is handed them.
  let tolerances = matching::current();
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
  counts.written = results.into_iter().filter(|written| *written).count();

  fs::create_dir_all(&staging)?;
  remove_dir_if_exists(&target)?;
  fs::rename(&staging, &target)?;
  tracing::info!(
    "Wrote {} and kept {} YMAP diffs in {}",
    counts.written,
    counts.reused,
    target.display()
  );
  Ok(counts)
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
  let Ok(reader) = fs::File::open(path) else {
    return false;
  };
  let Ok(inputs) = serde_json::from_reader::<_, DiffInputs>(BufReader::new(reader)) else {
    return false;
  };
  inputs.schema_version == YmapComparisonDiff::SCHEMA_VERSION
    && inputs.tolerances == Some(matching::current())
    && context.is_diff_current(file, &inputs.source, &inputs.vanilla).unwrap_or(false)
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
  #[ignore = "requires local asset/source-cache and asset/vanilla-cache"]
  fn writes_ymap_diffs_for_local_source_cache() {
    let asset = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset");
    let source_cache = asset.join("source-cache");
    let output = std::env::temp_dir().join(format!("ymap_diffs_{}", std::process::id()));
    let _ = fs::remove_dir_all(&output);
    for entry in walkdir::WalkDir::new(&source_cache) {
      let entry = entry.unwrap();
      let destination = output.join(entry.path().strip_prefix(&source_cache).unwrap());
      if entry.file_type().is_dir() {
        fs::create_dir_all(&destination).unwrap();
      } else {
        fs::copy(entry.path(), &destination).unwrap();
      }
    }

    let written = write_ymap_diffs(&output, &asset.join("vanilla"), &asset.join("vanilla-cache"))
      .unwrap()
      .written;
    let files = walkdir::WalkDir::new(output.join("diff"))
      .into_iter()
      .filter_map(std::result::Result::ok)
      .filter(|entry| entry.file_name().to_string_lossy().ends_with(".ymap.diff.json"))
      .count();
    println!("{written} diffs -> {}", output.join("diff").display());
    assert!(written > 0);
    assert_eq!(files, written);
    assert!(!output.join("diff.tmp").exists());

    // Nothing changed since, so every diff is kept.
    let rerun =
      write_ymap_diffs(&output, &asset.join("vanilla"), &asset.join("vanilla-cache")).unwrap();
    assert_eq!(
      rerun,
      YmapDiffCounts {
        written: 0,
        reused: written,
      }
    );
  }
}
