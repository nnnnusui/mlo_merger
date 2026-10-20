//! Bounds indexes of the vanilla, source and merged stream files, one per store.
//!
//! The stores change at different rates, so each has its own index, rebuilt only when the
//! metadata its files are listed from changes: the vanilla cache's after a game update, the
//! source cache's after a source-cache update, the merge output's after a merge.

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
  io::BufReader,
  path::{Path, PathBuf},
  time::Duration,
};

/// Bump when the indexed files, their bounds or the index layout change.
const SCHEMA_VERSION: u32 = 2;

/// Index files of an earlier layout, one per file type, removed once the store indexes exist.
const LEGACY_OUTPUTS: [&str; 2] = ["stream_file_rtree-ymap.json", "stream_file_rtree-ybn.json"];

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
  Vanilla,
  Source,
  Merged,
}

impl Store {
  const ALL: [Self; 3] = [Self::Vanilla, Self::Source, Self::Merged];

  pub(crate) fn name(self) -> &'static str {
    match self {
      Self::Vanilla => "vanilla",
      Self::Source => "source",
      Self::Merged => "merged",
    }
  }
}

/// Writes `stream_file_rtree-{vanilla,source,merged}.json`: RBush-compatible 2D boxes with
/// retained Z bounds for each store's YMAP and YBN files.
#[derive(Debug, Clone)]
pub(crate) struct GenerateRTree {
  /// Derived vanilla cache whose latest files are indexed.
  pub(crate) vanilla_cache_dir: PathBuf,
  /// Source cache whose inventoried files are indexed, where they are now.
  pub(crate) source_cache_dir: PathBuf,
  /// Merge output whose merged files are indexed.
  pub(crate) merged_dir: PathBuf,
  /// Where the indexes are written.
  pub(crate) output_dir: PathBuf,
  /// Rebuilds every index, even one whose store metadata is unchanged.
  pub(crate) force: bool,
}

/// What [`GenerateRTree::run`] did for one store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoreOutcome {
  pub(crate) store: Store,
  pub(crate) output: PathBuf,
  /// `true` when the index was written, `false` when it was already current.
  pub(crate) updated: bool,
}

impl GenerateRTree {
  /// Rebuilds the index of each store whose metadata changed since its index was written.
  ///
  /// A store without metadata is skipped with a warning, keeping any index it has.
  pub(crate) fn run(&self) -> Result<Vec<StoreOutcome>> {
    fs::create_dir_all(&self.output_dir)?;
    let working_dir = std::env::current_dir().and_then(|directory| directory.canonicalize()).ok();
    let mut outcomes = Vec::new();
    for store in Store::ALL {
      let info = self.cache_info_path(store);
      if !info.is_file() {
        eprintln!(
          "generate-rtree-json: {} metadata is missing; skipping {}",
          store.name(),
          info.display()
        );
        continue;
      }
      let output = self.output_dir.join(format!("stream_file_rtree-{}.json", store.name()));
      let input_path =
        display_path(&info, working_dir.as_deref()).to_string_lossy().replace('\\', "/");
      let previous = read_header(&output).filter(|header| header.input.path == input_path);
      let fingerprint = Fingerprint::read(
        &info,
        previous.as_ref().map(|header| fingerprint_of(&header.input)).as_ref(),
        false,
      )?;
      let input = RTreeInput {
        path: input_path,
        sha256: fingerprint.sha256,
        size: fingerprint.size,
        modified_seconds: fingerprint.modified_seconds,
        modified_nanos: fingerprint.modified_nanos,
      };
      let current = previous.is_some_and(|header| {
        header.schema_version == SCHEMA_VERSION
          && header.input.sha256 == input.sha256
          && header.input.size == input.size
      });
      if current && !self.force {
        eprintln!("generate-rtree-json: {} index is current", store.name());
        outcomes.push(StoreOutcome {
          store,
          output,
          updated: false,
        });
        continue;
      }
      let candidates = self.candidates(store, working_dir.as_deref())?;
      let index = index_store(store, input, candidates)?;
      crate::core::vanilla::write_json(&output, &index)?;
      outcomes.push(StoreOutcome {
        store,
        output,
        updated: true,
      });
    }
    if outcomes.len() == Store::ALL.len() {
      for legacy in LEGACY_OUTPUTS {
        let path = self.output_dir.join(legacy);
        if path.is_file() {
          fs::remove_file(&path)?;
          eprintln!("generate-rtree-json: removed index of the earlier layout {}", path.display());
        }
      }
    }
    Ok(outcomes)
  }

  /// The metadata of `store`, owned by the module that writes it.
  fn cache_info_path(
    &self,
    store: Store,
  ) -> PathBuf {
    match store {
      Store::Vanilla => crate::core::vanilla_cache::cache_info_path(&self.vanilla_cache_dir),
      Store::Source => crate::core::source_cache::cache_info_path(&self.source_cache_dir),
      Store::Merged => crate::core::merge::cache_info_path(&self.merged_dir),
    }
  }

  /// The YMAP and YBN files of `store`, as its metadata lists them.
  fn candidates(
    &self,
    store: Store,
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
    match store {
      Store::Vanilla => {
        for file in crate::core::vanilla_cache::latest_files(&self.vanilla_cache_dir)? {
          if let Some(mut found) = candidate(&file.name, display(&file.path), "vanilla-cache") {
            found.version = Some(file.version);
            candidates.push(found);
          }
        }
      }
      Store::Source => {
        for file in crate::core::source_cache::inventoried_files(&self.source_cache_dir)? {
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
      Store::Merged => {
        for file in crate::core::merge::merged_files(&self.merged_dir)? {
          if let Some(mut found) = candidate(&file.name, display(&file.path), "merged") {
            // Merged files are this tool's own output; one it cannot read is an error.
            found.strict = true;
            candidates.push(found);
          }
        }
      }
    }
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(candidates)
  }
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
    eprintln!("generate-rtree-json: indexing {total} {name} stream files");
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
      Err(error) if candidate.strict => return Err(error),
      Err(error) => {
        skipped += 1;
        progress.println(format!("Warning: skipped {}: {error}", candidate.path.display()));
      }
    }
    progress.inc(1);
    if log_progress && progress.position().is_multiple_of(1000) {
      eprintln!("generate-rtree-json: indexed {}/{total} {name} files", progress.position());
    }
  }
  progress.finish_and_clear();
  eprintln!(
    "generate-rtree-json: indexed {} {name} files ({} YMAP, {} YBN), skipped {skipped}",
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
    write(&source_cache.join("source_cache_info.json"), &source_info("first"));
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
      &merged.join("merge_cache_info.json"),
      &serde_json::to_vec(&json!({
        "format_version": 1, "algorithm_versions": {}, "generated_at": "now",
        "files": {"map.ymap": record(Some("ymap/map.ymap")), "other.ymap": record(None)}
      }))
      .unwrap(),
    );
    let output = root.join("read-cache");
    write(&output.join(LEGACY_OUTPUTS[0]), b"{}");
    let generate = |force| {
      GenerateRTree {
        vanilla_cache_dir: vanilla.clone(),
        source_cache_dir: source_cache.clone(),
        merged_dir: merged.clone(),
        output_dir: output.clone(),
        force,
      }
      .run()
      .unwrap()
      .into_iter()
      .map(|outcome| (outcome.store.name(), outcome.updated))
      .collect::<Vec<_>>()
    };

    assert_eq!(generate(false), [("vanilla", true), ("source", true), ("merged", true)]);
    assert!(!output.join(LEGACY_OUTPUTS[0]).exists());
    let index = |store: &str| read(&output.join(format!("stream_file_rtree-{store}.json")));
    let vanilla_index = index("vanilla");
    assert_eq!(vanilla_index["schemaVersion"], SCHEMA_VERSION);
    assert_eq!(vanilla_index["ymap"][0]["name"], "map.ymap");
    assert_eq!(vanilla_index["ymap"][0]["version"], "0001-update");
    assert_eq!(vanilla_index["ybn"][0]["name"], "collision.ybn");
    let source_index = index("source");
    let items = source_index["ymap"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|item| item["resource"] == "[maps]/resource"));
    let moved = items.iter().find(|item| item["name"] == "map.ymap").unwrap();
    assert!(moved["path"].as_str().unwrap().ends_with("source-cache/resources/resource/map.ymap"));
    assert!(moved["originalPath"].as_str().unwrap().ends_with("resource/stream/map.ymap"));
    let live = items.iter().find(|item| item["name"] == "custom.ymap").unwrap();
    assert!(live.get("originalPath").is_none());
    let merged_index = index("merged");
    assert_eq!(merged_index["fileCount"], 1);
    assert!(merged_index["ymap"][0]["path"].as_str().unwrap().ends_with("merged/ymap/map.ymap"));

    assert_eq!(generate(false), [("vanilla", false), ("source", false), ("merged", false)]);
    write(&source_cache.join("source_cache_info.json"), &source_info("second"));
    assert_eq!(generate(false), [("vanilla", false), ("source", true), ("merged", false)]);
    assert_eq!(generate(true), [("vanilla", true), ("source", true), ("merged", true)]);

    // A store without metadata keeps its index and is left out.
    fs::remove_file(merged.join("merge_cache_info.json")).unwrap();
    assert_eq!(generate(false), [("vanilla", false), ("source", false)]);
    assert!(output.join("stream_file_rtree-merged.json").is_file());
    fs::remove_dir_all(root).unwrap();
  }
}
