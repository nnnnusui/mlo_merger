//! Source stream inventory, vanilla matching, and conflict/load planning.

mod added;
mod check;
mod conflicts;
pub(crate) mod exclusions;
mod generate;
pub mod history;
mod inventory;
mod layout;
mod publication;
mod read;
mod restore;
mod streams;
mod types;
mod ymap_plan;

use std::path::{Path, PathBuf};

use crate::common::changes::StoreChanges;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub use added::{
  AddedYmapRelationships, added_ymap_relationships, empty_baseline_bytes, empty_baseline_file,
};
pub(crate) use check::check;
pub(crate) use read::{CACHE_INFO, SourceYmapIndex, load_source_ymap_index};
pub use read::{
  CachedFile, InventoriedFile, MergeInputs, MergeSourceFile, cache_info_path, cached_files,
  inventoried_files, load_merge_inputs, resource_name,
};
use read::{FORMAT_VERSION, read_metadata};
#[cfg(test)]
pub(crate) use read::{moved_source_paths, write_test_inventory};
pub use restore::restore_source_cache;
pub(crate) use streams::read_stream_bytes;

/// The directory of the diff documents below the source cache.
pub const DIFF_DIR: &str = "diff";

/// Where the source cache keeps the diff document of the stream file at `stream_path` of the
/// resource named `resource`, relative to the cache: `diff/{resource}/{stream_path}.diff.json`.
pub fn diff_document_path(
  resource: &str,
  stream_path: &str,
) -> PathBuf {
  Path::new(DIFF_DIR).join(diff_name(resource, stream_path))
}

/// [`diff_document_path`] relative to the `diff` directory.
fn diff_name(
  resource: &str,
  stream_path: &str,
) -> PathBuf {
  Path::new(resource).join(format!("{}.diff.json", stream_path.replace('\\', "/")))
}

/// Where the source cache keeps the diff of a resource's YMAP, relative to its `diff` directory,
/// given the resource's source directory and the file's path below it.
pub(crate) fn stream_file_diff_path(
  resource_source: &Path,
  file_path: &str,
) -> Result<PathBuf> {
  let resource = resource_source.file_name().ok_or("Resource has no name")?;
  Ok(diff_name(&resource.to_string_lossy(), layout::stream_relative_path(file_path)?))
}

struct Staging(PathBuf);

impl Drop for Staging {
  fn drop(&mut self) {
    let _ = crate::platform::fs::remove_dir_all(&self.0);
  }
}

/// Outcome of building or reusing a source cache.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SourceCacheResult {
  /// Whether changed inventories or reports were published.
  pub published: bool,
  /// Stream files the run moved into, replaced in, or archived or dropped from the cache,
  /// relative to it.
  pub changes: StoreChanges,
}

/// Builds or reuses a source processing inventory without generating diffs.
#[derive(Debug, Clone)]
pub struct BuildSourceCache {
  /// Root containing FiveM resources.
  pub source_dir: PathBuf,
  /// Output directory for resource inventories and conflict reports.
  pub output_dir: PathBuf,
  /// Raw versioned vanilla archive used for lineage metadata.
  pub vanilla_dir: PathBuf,
  /// Derived latest files and YMAP relationship index.
  pub vanilla_cache_dir: PathBuf,
  /// One resource name or source-relative path; `None` updates all resources.
  pub resource: Option<String>,
  /// Resource exclusions for this run; absent or empty means no exclusions.
  pub exclusions: Option<Vec<String>>,
  /// Rebuilds even when all inputs and upstream revisions are current.
  pub force: bool,
}

impl BuildSourceCache {
  /// Updates configured resources and exclusions, skipping unchanged inputs, then brings the
  /// cache's stream-file bounds index (`stream_file_rtree.json`) up to date.
  ///
  /// Exclusions match case-insensitive substrings of source-relative resource paths.
  /// Exclusions are not persisted; absent or empty means no exclusions. Excluded active
  /// cache files are retained under `_old/`.
  ///
  /// The vanilla cache must already be built; this does not update it.
  ///
  /// ```no_run
  /// use mlo_merger::core::source_cache::BuildSourceCache;
  /// BuildSourceCache {
  ///   source_dir: "asset/source".into(), output_dir: "asset/source-cache".into(),
  ///   vanilla_dir: "asset/vanilla".into(), vanilla_cache_dir: "asset/vanilla-cache".into(),
  ///   resource: Some("maps".into()), exclusions: None, force: false,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<SourceCacheResult> {
    generate::run(self)
  }
}

#[cfg(test)]
mod tests;
