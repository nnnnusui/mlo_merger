//! Bounds indexes of the vanilla, source and merged stream files, one in each store.
//!
//! Each store writes its own `stream_file_rtree.json` when it is built: the vanilla cache, the
//! source cache and the merge output. An index records the fingerprint of the metadata its
//! files were listed from and is rebuilt only when that metadata changed, so a step that reuses
//! its store also restores a missing or outdated index.

use super::{
  Result,
  file_from_position::{self, Candidate, display_path, stream_name},
};
use crate::common::fingerprint::Fingerprint;
#[cfg(not(test))]
use indicatif::ProgressDrawTarget;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
#[cfg(not(test))]
use std::io::IsTerminal;
use std::{
  fs,
  io::{BufReader, BufWriter, Write},
  path::{Path, PathBuf},
  time::Duration,
};

/// Bump when the indexed files, their bounds or the index layout change.
const SCHEMA_VERSION: u32 = 3;

/// File name of the index in each store directory.
pub(crate) const INDEX_FILE: &str = "stream_file_rtree.json";

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
struct RTreeIndex {
  #[serde(rename = "schemaVersion")]
  schema_version: u32,
  /// `vanilla`, `source` or `merged`.
  store: String,
  timestamp: String,
  /// The metadata the files were listed from; the index is rebuilt when it changes.
  input: RTreeInput,
  #[serde(rename = "fileCount")]
  file_count: usize,
  ymap: Vec<RTreeItem>,
  ybn: Vec<RTreeItem>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
struct RTreeInput {
  path: String,
  sha256: String,
  size: u64,
  #[serde(rename = "modifiedSeconds")]
  modified_seconds: u64,
  #[serde(rename = "modifiedNanos")]
  modified_nanos: u32,
}

/// The fields of an index read to decide whether it is current, without its items.
#[derive(Deserialize)]
struct Header {
  #[serde(rename = "schemaVersion")]
  schema_version: u32,
  input: RTreeInput,
}

#[derive(Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
struct RTreeItem {
  #[serde(rename = "minX")]
  min_x: f64,
  #[serde(rename = "minY")]
  min_y: f64,
  #[serde(rename = "minZ")]
  min_z: f64,
  #[serde(rename = "maxX")]
  max_x: f64,
  #[serde(rename = "maxY")]
  max_y: f64,
  #[serde(rename = "maxZ")]
  max_z: f64,
  name: String,
  path: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  resource: Option<String>,
  #[serde(rename = "originalPath", default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  original_path: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  version: Option<String>,
}

/// A store of stream files with its own index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Store {
  /// The derived vanilla cache's latest files.
  Vanilla,
  /// The source cache's inventoried files, where they are now.
  Source,
  /// The merge output's merged files.
  Merged,
}

impl Store {
  pub(crate) fn name(self) -> &'static str {
    match self {
      Self::Vanilla => "vanilla",
      Self::Source => "source",
      Self::Merged => "merged",
    }
  }

  /// The metadata of the store at `directory`, owned by the module that writes it.
  fn cache_info_path(
    self,
    directory: &Path,
  ) -> PathBuf {
    match self {
      Self::Vanilla => crate::core::vanilla_cache::cache_info_path(directory),
      Self::Source => crate::core::source_cache::cache_info_path(directory),
      Self::Merged => crate::core::merge::cache_info_path(directory),
    }
  }

  /// The YMAP and YBN files of the store at `directory`, as its metadata lists them.
  fn candidates(
    self,
    directory: &Path,
    working_dir: Option<&Path>,
  ) -> Result<Vec<Candidate>> {
    let display = |path: &Path| display_path(path, working_dir);
    let candidate = |name: &str, path: PathBuf, stage| {
      let (_, extension) = stream_name(Path::new(name))?;
      Some(Candidate {
        name: name.to_ascii_lowercase(),
        extension,
        path,
        stage,
        resource: None,
        original_path: None,
        version: None,
        strict: false,
      })
    };
    let mut candidates = Vec::new();
    match self {
      Self::Vanilla => {
        for file in crate::core::vanilla_cache::latest_files(directory)? {
          if let Some(mut found) = candidate(&file.name, display(&file.path), "vanilla-cache") {
            found.version = Some(file.version);
            candidates.push(found);
          }
        }
      }
      Self::Source => {
        for file in crate::core::source_cache::inventoried_files(directory)? {
          let moved = file.path != file.original_path;
          let stage = if moved { "source-cache" } else { "source" };
          if let Some(mut found) = candidate(&file.file_name, display(&file.path), stage) {
            found.resource = Some(file.resource);
            found.original_path =
              moved.then(|| display(&file.original_path).to_string_lossy().replace('\\', "/"));
            candidates.push(found);
          }
        }
      }
      Self::Merged => {
        for file in crate::core::merge::merged_files(directory)? {
          candidates.extend(candidate(&file.name, display(&file.path), "merged"));
        }
      }
    }
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(candidates)
  }
}

/// Writes the index of the store at `directory` to its `stream_file_rtree.json`, unless the
/// index already describes the store's current metadata; returns whether it was written.
///
/// Files whose bounds cannot be read are left out with a warning, so a store that was built
/// still gets an index. A store without metadata has no index to write.
pub(crate) fn update_store_index(
  store: Store,
  directory: &Path,
) -> Result<bool> {
  let info = store.cache_info_path(directory);
  if !info.is_file() {
    return Ok(false);
  }
  let output = directory.join(INDEX_FILE);
  let working_dir = std::env::current_dir().and_then(|directory| directory.canonicalize()).ok();
  let input_path = display_path(&info, working_dir.as_deref()).to_string_lossy().replace('\\', "/");
  let previous = read_header(&output).filter(|header| header.input.path == input_path);
  let fingerprint = Fingerprint::read(
    &info,
    previous.as_ref().map(|header| fingerprint_of(&header.input)).as_ref(),
    false,
  )?;
  let current = previous.is_some_and(|header| {
    header.schema_version == SCHEMA_VERSION
      && header.input.sha256 == fingerprint.sha256
      && header.input.size == fingerprint.size
  });
  if current {
    return Ok(false);
  }
  let input = RTreeInput {
    path: input_path,
    sha256: fingerprint.sha256,
    size: fingerprint.size,
    modified_seconds: fingerprint.modified_seconds,
    modified_nanos: fingerprint.modified_nanos,
  };
  let index = index_store(store, input, store.candidates(directory, working_dir.as_deref())?)?;
  let partial = directory.join(format!("{INDEX_FILE}.partial"));
  let mut writer = BufWriter::new(fs::File::create(&partial)?);
  serde_json::to_writer_pretty(&mut writer, &index)?;
  writer.flush()?;
  drop(writer);
  fs::rename(&partial, &output)?;
  tracing::info!("Wrote the {} stream-file R-tree index to {}", store.name(), output.display());
  Ok(true)
}

fn read_header(path: &Path) -> Option<Header> {
  serde_json::from_reader(BufReader::new(fs::File::open(path).ok()?)).ok()
}

fn fingerprint_of(input: &RTreeInput) -> Fingerprint {
  Fingerprint {
    sha256: input.sha256.clone(),
    size: input.size,
    modified_seconds: input.modified_seconds,
    modified_nanos: input.modified_nanos,
  }
}

/// Reads the bounds of each candidate of `store`.
fn index_store(
  store: Store,
  input: RTreeInput,
  candidates: Vec<Candidate>,
) -> Result<RTreeIndex> {
  #[cfg(not(test))]
  let log_progress = !std::io::stderr().is_terminal();
  #[cfg(test)]
  let log_progress = false;
  #[cfg(test)]
  let progress = ProgressBar::hidden();
  #[cfg(not(test))]
  let progress = ProgressBar::with_draw_target(Some(0), ProgressDrawTarget::stderr());
  let total = candidates.len();
  progress.set_length(total as u64);
  progress.set_style(
    ProgressStyle::with_template(
      "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
    )?
    .progress_chars("##-"),
  );
  progress.enable_steady_tick(Duration::from_millis(100));
  let name = store.name();
  if log_progress {
    tracing::info!("Indexing the bounds of {total} {name} stream files");
  }
  let mut ymap = Vec::new();
  let mut ybn = Vec::new();
  let mut skipped = 0usize;
  for candidate in candidates {
    progress.set_message(candidate.path.display().to_string());
    let items = if candidate.extension == "ymap" { &mut ymap } else { &mut ybn };
    match index_item(&candidate) {
      Ok(Some(item)) => items.push(item),
      Ok(None) => skipped += 1,
      Err(error) => {
        skipped += 1;
        progress.suspend(|| {
          tracing::warn!("Left {} out of the {name} index: {error}", candidate.path.display())
        });
      }
    }
    progress.inc(1);
    if log_progress && progress.position().is_multiple_of(1000) {
      tracing::info!("Indexed {}/{total} {name} stream files", progress.position());
    }
  }
  progress.finish_and_clear();
  tracing::info!(
    "Indexed the bounds of {} {name} stream files ({} YMAP, {} YBN), left out {skipped}",
    ymap.len() + ybn.len(),
    ymap.len(),
    ybn.len()
  );
  Ok(RTreeIndex {
    schema_version: SCHEMA_VERSION,
    store: name.into(),
    timestamp: chrono::Utc::now().to_rfc3339(),
    input,
    file_count: ymap.len() + ybn.len(),
    ymap,
    ybn,
  })
}

fn index_item(candidate: &Candidate) -> Result<Option<RTreeItem>> {
  let Some(bounds) = file_from_position::spatial_bounds(candidate)? else {
    return Ok(None);
  };
  Ok(Some(RTreeItem {
    min_x: bounds.minimum[0],
    min_y: bounds.minimum[1],
    min_z: bounds.minimum[2],
    max_x: bounds.maximum[0],
    max_y: bounds.maximum[1],
    max_z: bounds.maximum[2],
    name: candidate.name.clone(),
    path: candidate.path.to_string_lossy().replace('\\', "/"),
    resource: candidate.resource.clone(),
    original_path: candidate.original_path.clone(),
    version: candidate.version.clone(),
  }))
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  fn write(
    path: &Path,
    bytes: &[u8],
  ) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
  }

  fn read(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
  }

  #[test]
  fn each_store_is_indexed_again_only_when_its_metadata_changes() {
    let root = std::env::temp_dir().join(format!("rtree_stores_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let sample = |name: &str| {
      fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample").join(name)).unwrap()
    };
    let ymap = sample("sample.ymap");
    let ybn = crate::core::format::ybn::write_ybn(
      &crate::core::format::ybn::xml::read_xml(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/sample/ybn_conflicts/geometry_bvh.ybn.xml"
      )))
      .unwrap(),
    )
    .unwrap();
    let vanilla = root.join("vanilla-cache");
    write(&vanilla.join("objects/map.ymap"), &ymap);
    write(&vanilla.join("objects/collision.ybn"), &ybn);
    let vanilla_file =
      |object: &str| json!({"version": "0001-update", "sha256": "", "object": object});
    write(
      &vanilla.join("cache_info.json"),
      &serde_json::to_vec(&json!({
        "format_version": crate::core::vanilla_cache::CACHE_SCHEMA,
        "vanilla_manifest_sha256": "", "latest_version": "0001-update",
        "files": {
          "map.ymap": vanilla_file("objects/map.ymap"),
          "collision.ybn": vanilla_file("objects/collision.ybn"),
        }
      }))
      .unwrap(),
    );
    let source = root.join("source/resource");
    let source_cache = root.join("source-cache");
    write(&source.join("stream/custom.ymap"), &ymap);
    write(&source_cache.join("resources/resource/map.ymap"), &ymap);
    let source_file = |path: &str, name: &str, cached: Option<&str>| {
      json!({
        "path": path, "file_name": name, "sha256": "", "size": 0, "modified_seconds": 0,
        "modified_nanos": 0, "vanilla": null, "ymap_parent_hash": null, "metadata_error": null,
        "cached_path": cached
      })
    };
    let source_info = |generated_at: &str| {
      serde_json::to_vec(&json!({
        "format_version": 3, "source_dir": root.join("source"), "vanilla_dir": "",
        "vanilla_cache_dir": "", "vanilla_manifest_sha256": "", "vanilla_cache_revision": "",
        "latest_vanilla_version": "", "generated_at": generated_at, "source_inputs": {},
        "resource_checks": {},
        "resources": {"[maps]/resource": {"source": source, "files_by_format": {".ymap": [
          source_file("stream/custom.ymap", "custom.ymap", None),
          source_file("stream/map.ymap", "map.ymap", Some("resources/resource/map.ymap")),
        ]}}},
        "scanned_stream_file_count": 2, "conflict_count": 0,
        "ymap_load_plan": {"changed_source_parents": [], "additional_source_children": []},
        "outputs": []
      }))
      .unwrap()
    };
    write(&source_cache.join("cache_info.json"), &source_info("first"));
    let merged = root.join("merged");
    write(&merged.join("ymap/map.ymap"), &ymap);
    let record = |output: Option<&str>| {
      json!({
        "merge_sources": [], "vanilla": null, "dependencies": [], "dependency_fingerprint": "",
        "merged_at": "now",
        "output": output.map(|path| json!({"path": path, "fingerprint": {
          "sha256": "", "size": 0, "modified_seconds": 0, "modified_nanos": 0
        }}))
      })
    };
    write(
      &merged.join("cache_info.json"),
      &serde_json::to_vec(&json!({
        "format_version": 1, "algorithm_versions": {}, "generated_at": "now",
        "files": {"map.ymap": record(Some("ymap/map.ymap")), "other.ymap": record(None)}
      }))
      .unwrap(),
    );
    let stores =
      [(Store::Vanilla, &vanilla), (Store::Source, &source_cache), (Store::Merged, &merged)];
    let update = || {
      stores
        .iter()
        .map(|(store, directory)| (store.name(), update_store_index(*store, directory).unwrap()))
        .collect::<Vec<_>>()
    };

    assert_eq!(update(), [("vanilla", true), ("source", true), ("merged", true)]);
    let index = |directory: &Path| read(&directory.join(INDEX_FILE));
    let vanilla_index = index(&vanilla);
    assert_eq!(vanilla_index["schemaVersion"], SCHEMA_VERSION);
    assert_eq!(vanilla_index["store"], "vanilla");
    assert_eq!(vanilla_index["ymap"][0]["name"], "map.ymap");
    assert_eq!(vanilla_index["ymap"][0]["version"], "0001-update");
    assert_eq!(vanilla_index["ybn"][0]["name"], "collision.ybn");
    let source_index = index(&source_cache);
    let items = source_index["ymap"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|item| item["resource"] == "[maps]/resource"));
    let moved = items.iter().find(|item| item["name"] == "map.ymap").unwrap();
    assert!(moved["path"].as_str().unwrap().ends_with("source-cache/resources/resource/map.ymap"));
    assert!(moved["originalPath"].as_str().unwrap().ends_with("resource/stream/map.ymap"));
    let live = items.iter().find(|item| item["name"] == "custom.ymap").unwrap();
    assert!(live.get("originalPath").is_none());
    let merged_index = index(&merged);
    assert_eq!(merged_index["fileCount"], 1);
    assert!(merged_index["ymap"][0]["path"].as_str().unwrap().ends_with("merged/ymap/map.ymap"));

    // Only a store whose metadata changed, or whose index is missing, is indexed again.
    assert_eq!(update(), [("vanilla", false), ("source", false), ("merged", false)]);
    write(&source_cache.join("cache_info.json"), &source_info("second"));
    fs::remove_file(merged.join(INDEX_FILE)).unwrap();
    assert_eq!(update(), [("vanilla", false), ("source", true), ("merged", true)]);

    // A store without metadata has no index to write.
    fs::remove_file(merged.join("cache_info.json")).unwrap();
    fs::remove_file(merged.join(INDEX_FILE)).unwrap();
    assert!(!update_store_index(Store::Merged, &merged).unwrap());
    assert!(!merged.join(INDEX_FILE).exists());
    fs::remove_dir_all(root).unwrap();
  }
}
