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
mod restore;
mod streams;
mod types;
mod ymap_plan;

use crate::platform::fs;
use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Component, Path, PathBuf},
};

use self::types::{SourceCacheConflictReport, SourceCacheMetadata};
use crate::common::changes::StoreChanges;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub use added::{
  AddedYmapRelationships, added_ymap_relationships, empty_baseline_bytes, empty_baseline_file,
};
pub(crate) use check::check;
pub use restore::restore_source_cache;
pub(crate) use streams::read_stream_bytes;

/// Where the source cache keeps the diff of a resource's YMAP, relative to its `diff` directory:
/// `{resource directory name}/{stream-relative path}.diff.json`.
pub(crate) fn ymap_diff_path(
  resource_source: &Path,
  file_path: &str,
) -> Result<PathBuf> {
  let resource = resource_source.file_name().ok_or("Resource has no name")?;
  let relative = layout::stream_relative_path(file_path)?;
  Ok(Path::new(resource).join(format!("{relative}.diff.json")))
}

/// One source stream file selected for merge from the current source cache.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MergeSourceFile {
  /// Resource identifier recorded by the source inventory.
  pub resource: String,
  /// Absolute path to the source stream file.
  pub path: PathBuf,
  /// [`Self::path`] relative to the directory holding it: the source cache when the file was
  /// moved there, otherwise the source root.
  #[serde(default)]
  pub stored_path: String,
  /// Whether the file was moved into the source cache.
  #[serde(default)]
  pub moved: bool,
  /// Original resource stream path before extraction into the cache.
  pub original_path: PathBuf,
  /// [`Self::original_path`] relative to the source root.
  #[serde(default)]
  pub source_path: String,
  /// Lowercase stream filename.
  pub file_name: String,
  /// Path below the resource's `stream` directory, such as `vanilla/hei_ch1_11.ymap`.
  #[serde(default)]
  pub stream_path: String,
  /// The source cache's diff document of this YMAP against the latest vanilla; `None` for
  /// other formats.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub diff_path: Option<PathBuf>,
  /// Whether this is not a stream file but the changes the resource's YMAPs make, carried over
  /// to a newer generation of the vanilla YMAP [`Self::file_name`] (a `YmapPropagatedDiff` at
  /// [`Self::path`]); the resource does not ship that YMAP.
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub propagated: bool,
}

/// Source inputs and vanilla YMAP closure selected by the source-cache analysis.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeInputs {
  /// Changed or conflicting source YMAPs with a latest vanilla baseline, and every YMAP a
  /// resource adds, named in [`Self::added_ymaps`].
  pub ymap: Vec<MergeSourceFile>,
  /// Conflicting source YBNs with a latest vanilla baseline.
  pub ybn: Vec<MergeSourceFile>,
  /// Changes of source YMAPs carried over to newer generations of their vanilla YMAPs, one per
  /// resource and vanilla YMAP; see [`MergeSourceFile::propagated`].
  pub propagated: Vec<MergeSourceFile>,
  /// Latest vanilla YMAP filenames required by the source load plan.
  pub vanilla_ymaps_to_read: BTreeSet<String>,
  /// The lowercase file names of the YMAPs resources add, which replace no vanilla YMAP and
  /// whose diff documents are taken against an empty YMAP ([`empty_baseline_file`]).
  pub added_ymaps: BTreeSet<String>,
}

/// `path` with `/` separators, as relative paths are recorded.
fn slash_path(path: &Path) -> String {
  path.to_string_lossy().replace('\\', "/")
}

/// The [`SourceCacheMetadata::format_version`] this version writes.
const FORMAT_VERSION: u32 = 5;

/// Reads the source cache's inventory, rejecting layouts this version cannot read.
///
/// Each resource's `source` is set to its ID, the path relative to the source root, which
/// earlier formats recorded as an absolute path.
fn read_metadata(cache_dir: &Path) -> Result<SourceCacheMetadata> {
  let path = cache_info_path(cache_dir);
  let mut metadata: SourceCacheMetadata = crate::common::json::read_json(&path)
    .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
  if !(2..=FORMAT_VERSION).contains(&metadata.format_version) {
    return Err(format!("Unsupported source cache schema {}", metadata.format_version).into());
  }
  for (id, resource) in &mut metadata.resources {
    let relative = PathBuf::from(id);
    if relative.components().any(|component| !matches!(component, Component::Normal(_))) {
      return Err(format!("Unsafe source-cache resource ID {id}").into());
    }
    resource.source = relative;
  }
  Ok(metadata)
}

/// A stream file the source cache moved out of its resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedFile {
  /// Path relative to the cache directory: `resources/{resource name}/{stream-relative path}`.
  pub cached_path: String,
  /// Original path relative to the source root.
  pub source_path: String,
}

/// The files the inventory records as moved into the cache, sorted by cached path.
///
/// Only recorded files are listed: anything else under `resources/` is not part of the cache.
/// A recorded file that is missing is an error, as the cache no longer matches its inventory.
///
/// ```no_run
/// let files = mlo_merger::core::source_cache::cached_files(
///   std::path::Path::new("asset/source-cache"),
/// )?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn cached_files(cache_dir: &Path) -> Result<Vec<CachedFile>> {
  let metadata = read_metadata(cache_dir)?;
  let safe =
    |path: &Path| path.components().all(|component| matches!(component, Component::Normal(_)));
  let mut files = Vec::new();
  for resource in metadata.resources.values() {
    for file in resource.files_by_format.values().flatten() {
      let Some(cached) = &file.cached_path else {
        continue;
      };
      // Archived history is not part of the active cache.
      if cached.starts_with("_old/") {
        continue;
      }
      if !safe(Path::new(cached)) || !cached.starts_with("resources/") {
        return Err(format!("Unsafe source-cache path {cached}").into());
      }
      if !fs::is_file(cache_dir.join(cached)) {
        return Err(
          format!("Recorded cached file is missing: {cached}; rebuild the source cache").into(),
        );
      }
      let source = resource.source.join(&file.path);
      if !safe(&source) {
        return Err("Unsafe original moved source path".into());
      }
      let source_path = slash_path(&source);
      if source_path.contains(['\r', '\n']) {
        return Err("Moved source path contains a line break".into());
      }
      files.push(CachedFile {
        cached_path: cached.clone(),
        source_path,
      });
    }
  }
  files.sort_by(|a, b| a.cached_path.cmp(&b.cached_path));
  Ok(files)
}

/// The original source-relative paths of [`cached_files`].
#[cfg(test)]
pub(crate) fn moved_source_paths(cache_dir: &Path) -> BTreeSet<String> {
  cached_files(cache_dir).unwrap().into_iter().map(|file| file.source_path).collect()
}

/// Writes an inventory recording each of `cached` (`resources/{resource}/{stream path}`) as
/// moved from `{resource}/stream/{stream path}` in the source root, for tests of its readers.
#[cfg(test)]
pub(crate) fn write_test_inventory(
  cache_dir: &Path,
  cached: &[&str],
) {
  use types::{ResourceInventory, SourceFile, YmapLoadPlan};
  let mut resources = BTreeMap::<String, ResourceInventory>::new();
  for cached_path in cached {
    let (resource, stream_path) =
      cached_path.strip_prefix("resources/").unwrap().split_once('/').unwrap();
    let path = format!("stream/{stream_path}");
    let file_name = stream_path.rsplit('/').next().unwrap().to_string();
    let extension = format!(".{}", file_name.rsplit('.').next().unwrap().to_ascii_lowercase());
    resources
      .entry(resource.to_string())
      .or_insert_with(|| ResourceInventory {
        source: PathBuf::from(resource),
        files_by_format: BTreeMap::new(),
      })
      .files_by_format
      .entry(extension)
      .or_default()
      .push(SourceFile {
        path,
        file_name,
        sha256: String::new(),
        size: 0,
        modified_seconds: 0,
        modified_nanos: 0,
        vanilla: None,
        ymap_parent_hash: None,
        metadata_error: None,
        cached_path: Some(cached_path.to_string()),
      });
  }
  let metadata = SourceCacheMetadata {
    format_version: FORMAT_VERSION,
    vanilla_manifest_sha256: String::new(),
    vanilla_cache_revision: String::new(),
    latest_vanilla_version: String::new(),
    generated_at: String::new(),
    source_inputs: BTreeMap::new(),
    resource_checks: BTreeMap::new(),
    resources,
    scanned_stream_file_count: 0,
    conflict_count: 0,
    ymap_load_plan: YmapLoadPlan {
      changed_source_parents: BTreeSet::new(),
      additional_source_children: BTreeSet::new(),
    },
    outputs: BTreeSet::new(),
  };
  crate::platform::fs::create_dir_all(cache_dir).unwrap();
  crate::platform::fs::write(cache_info_path(cache_dir), serde_json::to_vec(&metadata).unwrap())
    .unwrap();
}

/// Recorded source YMAP paths and the vanilla version the source cache was generated against.
pub(crate) struct SourceYmapIndex {
  pub(crate) latest_vanilla_version: String,
  /// Paths keyed by resource name (its directory name, as under `resources/`) and lowercase
  /// file name; a resource may hold several YMAPs with the same name in different stream
  /// directories.
  pub(crate) paths: BTreeMap<(String, String), Vec<PathBuf>>,
  /// The lowercase file names of the YMAPs resources add, compared with an empty YMAP.
  pub(crate) added: BTreeSet<String>,
}

/// Indexes the YMAPs recorded in a generated source cache by resource name and file name; files
/// that were not moved into the cache are in their resources below `source_dir`.
pub(crate) fn load_source_ymap_index(
  cache_dir: &Path,
  source_dir: &Path,
) -> Result<SourceYmapIndex> {
  let metadata = read_metadata(cache_dir)?;
  let added = added::added_names(&metadata);
  let mut paths = BTreeMap::<_, Vec<_>>::new();
  for resource in metadata.resources.into_values() {
    let resource_name =
      resource.source.file_name().ok_or("Resource has no name")?.to_string_lossy().into_owned();
    for file in resource.files_by_format.get(".ymap").into_iter().flatten() {
      let path = file.cached_path.as_ref().map_or_else(
        || source_dir.join(&resource.source).join(&file.path),
        |path| cache_dir.join(path),
      );
      paths
        .entry((resource_name.clone(), file.file_name.to_ascii_lowercase()))
        .or_default()
        .push(path);
    }
  }
  Ok(SourceYmapIndex {
    latest_vanilla_version: metadata.latest_vanilla_version,
    paths,
    added,
  })
}

/// A stream file of a resource, as the source cache recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoriedFile {
  /// Resource identifier relative to the source root, including bracket groups.
  pub resource: String,
  /// Resource directory name.
  pub resource_name: String,
  /// Path below the resource's `stream` directory.
  pub stream_path: String,
  /// Stream filename.
  pub file_name: String,
  /// Where the file is now: in the cache when it was moved there, otherwise in the resource.
  pub path: PathBuf,
  /// [`Self::path`] relative to the directory holding it: the source cache when the file was
  /// moved there, otherwise the source root.
  pub stored_path: String,
  /// Whether the file was moved into the source cache.
  pub moved: bool,
  /// Where the file belongs, relative to the source root.
  pub source_path: String,
}

/// File name of the source cache's inventory.
pub(crate) const CACHE_INFO: &str = "cache_info.json";

/// The source cache's inventory, written whenever the cache is generated, so it changes with
/// every update of the cached or inventoried files.
pub fn cache_info_path(cache_dir: &Path) -> PathBuf {
  cache_dir.join(CACHE_INFO)
}

/// Lists every stream file the source cache inventoried, without rescanning source streams;
/// files that were not moved into the cache are in their resources below `source_dir`.
pub fn inventoried_files(
  cache_dir: &Path,
  source_dir: &Path,
) -> Result<Vec<InventoriedFile>> {
  let metadata = read_metadata(cache_dir)?;
  let mut files = Vec::new();
  for (resource, inventory) in metadata.resources {
    let resource_name =
      inventory.source.file_name().and_then(|name| name.to_str()).unwrap_or(&resource).to_string();
    for file in inventory.files_by_format.into_values().flatten() {
      let source_path = slash_path(&inventory.source.join(&file.path));
      let (path, stored_path) = match &file.cached_path {
        Some(cached) => (cache_dir.join(cached), cached.clone()),
        None => (source_dir.join(&source_path), source_path.clone()),
      };
      files.push(InventoriedFile {
        resource: resource.clone(),
        resource_name: resource_name.clone(),
        stream_path: layout::stream_relative_path(&file.path).unwrap_or(&file.path).to_string(),
        moved: file.cached_path.is_some(),
        path,
        stored_path,
        source_path,
        file_name: file.file_name,
      });
    }
  }
  Ok(files)
}

/// Loads merge inputs from a generated source cache without rescanning source streams; files
/// that were not moved into the cache are in their resources below `source_dir`.
pub fn load_merge_inputs(
  cache_dir: &Path,
  source_dir: &Path,
) -> Result<MergeInputs> {
  let metadata = read_metadata(cache_dir)?;
  let conflicts: SourceCacheConflictReport =
    crate::common::json::read_json(cache_dir.join("stream_conflicts.json"))?;
  let conflict_paths = [".ymap", ".ybn"]
    .into_iter()
    .map(|extension| {
      let paths = conflicts
        .report
        .conflicts
        .get(extension)
        .into_iter()
        .flatten()
        .flat_map(|conflict| conflict.paths.iter())
        .map(|path| path.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
      (extension, paths)
    })
    .collect::<BTreeMap<_, _>>();
  let required_ymap_sources = metadata
    .ymap_load_plan
    .changed_source_parents
    .iter()
    .chain(&metadata.ymap_load_plan.additional_source_children)
    .map(|source| (source.resource.clone(), source.source_path.to_ascii_lowercase()))
    .collect::<BTreeSet<_>>();
  let vanilla_ymaps_to_read =
    crate::common::json::read_json(cache_dir.join("vanilla_ymaps_to_read.json"))?;
  let mut inputs = MergeInputs {
    vanilla_ymaps_to_read,
    ..MergeInputs::default()
  };
  for (resource_id, resource) in metadata.resources {
    for (extension, files) in resource.files_by_format {
      let selected = match extension.as_str() {
        ".ymap" => &mut inputs.ymap,
        ".ybn" => &mut inputs.ybn,
        _ => continue,
      };
      for file in files {
        // Every added YMAP is merged against its baseline; one whose diff could not be taken,
        // as it cannot be read, is left as it is.
        let added = added::is_added_ymap(&extension, &file)
          && file.cached_path.is_some()
          && fs::is_file(
            cache_dir.join("diff").join(ymap_diff_path(&resource.source, &file.path)?),
          );
        if file.vanilla.is_none() && !added {
          continue;
        }
        if added {
          inputs.added_ymaps.insert(file.file_name.to_ascii_lowercase());
        }
        let relative_source = slash_path(&resource.source.join(&file.path));
        let original_path = source_dir.join(&relative_source);
        let (source_path, stored_path) = match &file.cached_path {
          Some(cached) => (cache_dir.join(cached), cached.clone()),
          None => (original_path.clone(), relative_source.clone()),
        };
        let source_relative = if conflicts.source_conflicts.is_some() {
          file.cached_path.as_ref().map(|path| path.to_ascii_lowercase())
        } else {
          Some(relative_source.to_ascii_lowercase())
        };
        let conflict = conflict_paths
          .get(extension.as_str())
          .is_some_and(|paths| source_relative.as_ref().is_some_and(|path| paths.contains(path)));
        let required_child = extension == ".ymap"
          && required_ymap_sources.contains(&(resource_id.clone(), file.path.to_ascii_lowercase()));
        let differs = file.vanilla.as_ref().is_none_or(|vanilla| !vanilla.content_matches);
        let selected_for_merge = match extension.as_str() {
          ".ymap" => differs || conflict || required_child,
          ".ybn" => conflict,
          _ => false,
        };
        if selected_for_merge {
          let diff_path = (extension == ".ymap")
            .then(|| ymap_diff_path(&resource.source, &file.path))
            .transpose()?
            .map(|path| cache_dir.join("diff").join(path));
          selected.push(MergeSourceFile {
            resource: resource_id.clone(),
            path: source_path,
            stored_path,
            moved: file.cached_path.is_some(),
            original_path,
            source_path: relative_source,
            file_name: file.file_name,
            stream_path: layout::stream_relative_path(&file.path).unwrap_or(&file.path).to_string(),
            diff_path,
            propagated: false,
          });
        }
      }
    }
  }
  for document in
    generate::propagate::read_record(cache_dir).map(|record| record.documents).unwrap_or_default()
  {
    let path = cache_dir.join(&document.path);
    inputs.propagated.push(MergeSourceFile {
      resource: document.resource,
      path: path.clone(),
      stored_path: document.path,
      moved: false,
      original_path: PathBuf::new(),
      source_path: String::new(),
      file_name: document.file_name,
      stream_path: String::new(),
      diff_path: Some(path),
      propagated: true,
    });
  }
  inputs.propagated.sort_by(|left, right| left.path.cmp(&right.path));
  inputs.ymap.sort_by(|left, right| left.path.cmp(&right.path));
  inputs.ybn.sort_by(|left, right| left.path.cmp(&right.path));
  Ok(inputs)
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
