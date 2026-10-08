use super::{
  FindEntity, Input, MergeCache, Result, aabb_intersects_radius, read_input, search_inputs,
};
use crate::core::format::{
  gamefile::{
    meta_resource::{MetaResource, MetaStructureEntry, jenk_hash},
    meta_xml::ymap_to_model_with_entities_from_meta,
    resource_file::Rsc7Resource,
  },
  ybn::read_ybn,
};
#[cfg(not(test))]
use indicatif::ProgressDrawTarget;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
#[cfg(not(test))]
use std::io::IsTerminal;
use std::{
  collections::{BTreeMap, HashMap, HashSet},
  path::{Component, Path, PathBuf},
  time::Duration,
};
use walkdir::WalkDir;

#[derive(Serialize)]
#[serde(rename = "FileFromPositionSearch")]
struct SearchResult {
  #[serde(rename = "@position")]
  position: String,
  #[serde(rename = "File")]
  files: Vec<FileResult>,
}

#[derive(Serialize)]
struct FileResult {
  #[serde(rename = "@name")]
  name: String,
  #[serde(rename = "@type")]
  kind: &'static str,
  #[serde(rename = "@stage")]
  stage: &'static str,
  #[serde(rename = "@path")]
  path: String,
  #[serde(rename = "@resource", skip_serializing_if = "Option::is_none")]
  resource: Option<String>,
  #[serde(rename = "@originalPath", skip_serializing_if = "Option::is_none")]
  original_path: Option<String>,
  #[serde(rename = "@version", skip_serializing_if = "Option::is_none")]
  version: Option<String>,
}

pub(super) struct Candidate {
  pub(super) name: String,
  pub(super) extension: &'static str,
  pub(super) path: PathBuf,
  pub(super) stage: &'static str,
  pub(super) resource: Option<String>,
  pub(super) original_path: Option<String>,
  pub(super) version: Option<String>,
  pub(super) input: Option<Input>,
  pub(super) strict: bool,
}

#[derive(Deserialize)]
struct SourceCacheIndex {
  format_version: u32,
  resources: BTreeMap<String, SourceResource>,
}

#[derive(Deserialize)]
struct SourceResource {
  source: PathBuf,
  files_by_format: BTreeMap<String, Vec<SourceFile>>,
}

#[derive(Deserialize)]
struct SourceFile {
  path: PathBuf,
  file_name: String,
  cached_path: Option<PathBuf>,
}

#[derive(Deserialize)]
struct VanillaCacheIndex {
  format_version: u32,
  files: BTreeMap<String, VanillaCacheFile>,
}

#[derive(Deserialize)]
struct VanillaCacheFile {
  version: String,
  object: PathBuf,
}

pub(super) fn run(
  search: &FindEntity,
  position: [f64; 3],
) -> Result<String> {
  #[cfg(not(test))]
  let log_progress = !std::io::stderr().is_terminal();
  #[cfg(test)]
  let log_progress = false;
  #[cfg(test)]
  let progress = ProgressBar::hidden();
  #[cfg(not(test))]
  let progress = ProgressBar::with_draw_target(Some(0), ProgressDrawTarget::stderr());
  progress.set_style(ProgressStyle::with_template("{spinner:.green} {msg}")?);
  progress.enable_steady_tick(Duration::from_millis(100));
  progress.set_message("Collecting file-from-position inputs");
  if log_progress {
    eprintln!("file-from-position: collecting search inputs");
  }
  let candidates = collect_candidates(search, &progress)?;
  let total = candidates.len();
  progress.set_length(total as u64);
  if log_progress {
    eprintln!("file-from-position: checking {total} files");
  }
  progress.set_style(
    ProgressStyle::with_template(
      "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
    )?
    .progress_chars("##-"),
  );
  let mut files = Vec::new();
  #[cfg(not(test))]
  let mut skipped = 0usize;
  for candidate in candidates {
    progress.set_message(candidate.path.display().to_string());
    match matching_file(&candidate, position) {
      Ok(Some(file)) => files.push(file),
      Ok(None) => {}
      Err(error) if candidate.strict => return Err(error),
      Err(error) => {
        #[cfg(not(test))]
        {
          skipped += 1;
        }
        progress.println(format!("Warning: skipped {}: {error}", candidate.path.display()));
      }
    }
    progress.inc(1);
    if log_progress && progress.position().is_multiple_of(1000) {
      eprintln!("file-from-position: checked {}/{} files", progress.position(), total);
    }
  }
  #[cfg(not(test))]
  let checked = progress.position();
  progress.finish_and_clear();
  #[cfg(not(test))]
  eprintln!(
    "file-from-position: checked {checked} files, found {}, skipped {skipped}",
    files.len()
  );
  let result = SearchResult {
    position: format!("{},{},{}", position[0], position[1], position[2]),
    files,
  };
  let mut xml = String::new();
  let mut serializer = quick_xml::se::Serializer::new(&mut xml);
  serializer.indent(' ', 2);
  result.serialize(serializer)?;
  Ok(xml)
}

pub(super) fn collect_candidates(
  search: &FindEntity,
  progress: &ProgressBar,
) -> Result<Vec<Candidate>> {
  collect_candidates_from(
    search,
    progress,
    Path::new("asset/source"),
    Path::new("asset/source-cache"),
    Path::new("asset/vanilla-cache"),
  )
}

fn collect_candidates_from(
  search: &FindEntity,
  progress: &ProgressBar,
  source_dir: &Path,
  source_cache_dir: &Path,
  vanilla_cache_dir: &Path,
) -> Result<Vec<Candidate>> {
  let filter = super::filename_filter(search.filter.as_deref())?;
  let mut candidates = Vec::new();
  let mut paths = HashSet::new();
  for extension in ["ymap", "ybn"] {
    if search.diff_all && !search.merged_dir.is_dir() {
      progress
        .println(format!("Warning: merged directory is missing: {}", search.merged_dir.display()));
      continue;
    }
    let missing_provenance =
      search.diff_all && !search.merged_dir.join("merge_cache_info.json").is_file();
    let merged_only;
    let search_config = if missing_provenance {
      merged_only = FindEntity {
        diff_all: false,
        ..search.clone()
      };
      &merged_only
    } else {
      search
    };
    if missing_provenance {
      progress.println("Warning: merge provenance is missing; scanning available stores only");
    }
    let (merged_paths, cache, names) = search_inputs(search_config, extension)?;
    for name in names {
      if let Some(path) = merged_paths.get(&name) {
        push_candidate(
          &mut candidates,
          &mut paths,
          Candidate {
            name: name.clone(),
            extension,
            path: path.clone(),
            stage: "merged",
            resource: None,
            original_path: None,
            version: None,
            input: None,
            strict: true,
          },
        );
      }
      if let Some(cache) = &cache {
        append_cached_candidates(&mut candidates, &mut paths, &name, extension, cache)?;
      }
    }
  }
  if search.diff_all {
    collect_source_candidates(&mut candidates, &mut paths, source_dir, filter.as_ref(), progress)?;
    collect_source_cache_candidates(
      &mut candidates,
      &mut paths,
      source_cache_dir,
      filter.as_ref(),
      progress,
    )?;
    collect_vanilla_candidates(
      &mut candidates,
      &mut paths,
      vanilla_cache_dir,
      filter.as_ref(),
      progress,
    )?;
  }
  Ok(candidates)
}

fn append_cached_candidates(
  candidates: &mut Vec<Candidate>,
  paths: &mut HashSet<PathBuf>,
  name: &str,
  extension: &'static str,
  cache: &MergeCache,
) -> Result<()> {
  let record =
    cache.files.get(name).ok_or_else(|| format!("No pre-merge provenance recorded for {name}"))?;
  if let Some(input) = &record.vanilla {
    push_candidate(candidates, paths, provenance_candidate(name, extension, input, "vanilla"));
  }
  for input in &record.merge_sources {
    push_candidate(candidates, paths, provenance_candidate(name, extension, input, "source"));
  }
  Ok(())
}

fn provenance_candidate(
  name: &str,
  extension: &'static str,
  input: &Input,
  stage: &'static str,
) -> Candidate {
  Candidate {
    name: name.to_string(),
    extension,
    path: input.path.clone(),
    stage,
    resource: input.resource.clone(),
    original_path: input
      .original_path
      .as_ref()
      .map(|path| path.to_string_lossy().replace('\\', "/")),
    version: None,
    input: Some(input.clone()),
    strict: true,
  }
}

fn collect_source_candidates(
  candidates: &mut Vec<Candidate>,
  paths: &mut HashSet<PathBuf>,
  source_dir: &Path,
  filter: Option<&globset::GlobMatcher>,
  progress: &ProgressBar,
) -> Result<()> {
  if !source_dir.is_dir() {
    progress.println(format!("Warning: source directory is missing: {}", source_dir.display()));
    return Ok(());
  }
  for entry in WalkDir::new(source_dir).follow_links(false) {
    let entry = match entry {
      Ok(entry) => entry,
      Err(error) => {
        progress.println(format!("Warning: {error}"));
        continue;
      }
    };
    if !entry.file_type().is_file() {
      continue;
    }
    let path = entry.path();
    let Some((name, extension)) = stream_name(path) else {
      continue;
    };
    if !filter_matches(filter, &name) {
      continue;
    }
    let resource = path
      .strip_prefix(source_dir)
      .ok()
      .and_then(|relative| relative.components().next())
      .and_then(|component| match component {
        Component::Normal(name) => name.to_str(),
        _ => None,
      })
      .map(str::to_string);
    push_candidate(
      candidates,
      paths,
      Candidate {
        name,
        extension,
        path: path.to_path_buf(),
        stage: "source",
        resource,
        original_path: None,
        version: None,
        input: None,
        strict: false,
      },
    );
  }
  Ok(())
}

fn collect_source_cache_candidates(
  candidates: &mut Vec<Candidate>,
  paths: &mut HashSet<PathBuf>,
  cache_root: &Path,
  filter: Option<&globset::GlobMatcher>,
  progress: &ProgressBar,
) -> Result<()> {
  let metadata_path = cache_root.join("source_cache_info.json");
  if !metadata_path.is_file() {
    return Ok(());
  }
  let index: SourceCacheIndex = serde_json::from_reader(std::fs::File::open(metadata_path)?)?;
  if !matches!(index.format_version, 2 | 3) {
    return Err(format!("Unsupported source cache schema {}", index.format_version).into());
  }
  for (resource_id, resource) in index.resources {
    for (format, files) in resource.files_by_format {
      let extension = match format.as_str() {
        ".ymap" => "ymap",
        ".ybn" => "ybn",
        _ => continue,
      };
      for file in files {
        if !filter_matches(filter, &file.file_name.to_ascii_lowercase()) {
          continue;
        }
        let Some(cached_path) = file.cached_path else {
          continue;
        };
        if !safe_relative(&cached_path) {
          return Err("Unsafe source-cache artifact path".into());
        }
        let original_path = resource.source.join(&file.path);
        if original_path.is_file() {
          continue;
        }
        let path = cache_root.join(cached_path);
        if !path.is_file() {
          progress
            .println(format!("Warning: source-cache artifact is missing: {}", path.display()));
          continue;
        }
        push_candidate(
          candidates,
          paths,
          Candidate {
            name: file.file_name.to_ascii_lowercase(),
            extension,
            path,
            stage: "source-cache",
            resource: Some(resource_id.clone()),
            original_path: Some(original_path.to_string_lossy().replace('\\', "/")),
            version: None,
            input: None,
            strict: false,
          },
        );
      }
    }
  }
  Ok(())
}

fn collect_vanilla_candidates(
  candidates: &mut Vec<Candidate>,
  paths: &mut HashSet<PathBuf>,
  cache_root: &Path,
  filter: Option<&globset::GlobMatcher>,
  progress: &ProgressBar,
) -> Result<()> {
  let manifest_path = cache_root.join("cache_info.json");
  if !manifest_path.is_file() {
    progress
      .println(format!("Warning: vanilla-cache manifest is missing: {}", manifest_path.display()));
    return Ok(());
  }
  let index: VanillaCacheIndex = serde_json::from_reader(std::fs::File::open(manifest_path)?)?;
  if index.format_version != 2 {
    return Err(format!("Unsupported vanilla-cache schema {}", index.format_version).into());
  }
  for (name, file) in index.files {
    let Some((_, extension)) = stream_name(Path::new(&name)) else {
      continue;
    };
    if !filter_matches(filter, &name.to_ascii_lowercase()) {
      continue;
    }
    if !safe_relative(&file.object) {
      return Err("Unsafe vanilla-cache artifact path".into());
    }
    let path = cache_root.join(file.object);
    if !path.is_file() {
      progress.println(format!("Warning: vanilla-cache artifact is missing: {}", path.display()));
      continue;
    }
    push_candidate(
      candidates,
      paths,
      Candidate {
        name: name.to_ascii_lowercase(),
        extension,
        path,
        stage: "vanilla-cache",
        resource: None,
        original_path: None,
        version: Some(file.version),
        input: None,
        strict: false,
      },
    );
  }
  Ok(())
}

fn push_candidate(
  candidates: &mut Vec<Candidate>,
  paths: &mut HashSet<PathBuf>,
  candidate: Candidate,
) {
  if paths.insert(candidate.path.clone()) {
    candidates.push(candidate);
  }
}

fn stream_name(path: &Path) -> Option<(String, &'static str)> {
  let name = path.file_name()?.to_str()?.to_ascii_lowercase();
  let extension = path.extension()?.to_str()?.to_ascii_lowercase();
  let extension = match extension.as_str() {
    "ymap" => "ymap",
    "ybn" => "ybn",
    _ => return None,
  };
  Some((name, extension))
}

fn safe_relative(path: &Path) -> bool {
  !path.as_os_str().is_empty()
    && !path.to_string_lossy().contains(['\\', ':'])
    && path.components().all(|component| matches!(component, Component::Normal(_)))
}

fn filter_matches(
  filter: Option<&globset::GlobMatcher>,
  name: &str,
) -> bool {
  filter.is_none_or(|filter| filter.is_match(name))
}

fn matching_file(
  candidate: &Candidate,
  position: [f64; 3],
) -> Result<Option<FileResult>> {
  let bytes = read_input(&candidate.path, candidate.stage, candidate.input.as_ref())?;
  let position = position.map(|value| value as f32 as f64);
  let matched = match candidate.extension {
    "ymap" => {
      let resource = Rsc7Resource::decode(&bytes)?;
      let meta = MetaResource::parse(&resource)?;
      match ymap_extents(&meta)? {
        Some((minimum, maximum)) => {
          aabb_intersects_radius(position, 0.0, minimum, maximum).unwrap_or(false)
        }
        None => {
          let (model, _) = ymap_to_model_with_entities_from_meta(&meta, &HashMap::new())?;
          let minimum = [
            model.entities_extents_min.x as f64,
            model.entities_extents_min.y as f64,
            model.entities_extents_min.z as f64,
          ];
          let maximum = [
            model.entities_extents_max.x as f64,
            model.entities_extents_max.y as f64,
            model.entities_extents_max.z as f64,
          ];
          aabb_intersects_radius(position, 0.0, minimum, maximum).unwrap_or(false)
        }
      }
    }
    "ybn" => {
      let root = read_ybn(&bytes)?;
      super::ybn::bounds_overlap(&root, position, 0.0)?
    }
    _ => unreachable!(),
  };
  Ok(matched.then(|| FileResult {
    name: candidate.name.clone(),
    kind: candidate.extension,
    stage: candidate.stage,
    path: candidate.path.to_string_lossy().replace('\\', "/"),
    resource: candidate.resource.clone(),
    original_path: candidate.original_path.clone(),
    version: candidate.version.clone(),
  }))
}

pub(super) fn ymap_extents(meta: &MetaResource) -> Result<Option<([f64; 3], [f64; 3])>> {
  let Some(root_index) = meta.root_block_index.checked_sub(1).map(|index| index as usize) else {
    return Ok(None);
  };
  let Some(root) = meta.data_blocks.get(root_index) else {
    return Ok(None);
  };
  let Some(schema) =
    meta.structures.iter().find(|schema| schema.name_hash == root.structure_name_hash)
  else {
    return Ok(None);
  };
  let Some(minimum) =
    read_position_field(&root.data, &schema.entries, jenk_hash("entitiesExtentsMin"))?
  else {
    return Ok(None);
  };
  let Some(maximum) =
    read_position_field(&root.data, &schema.entries, jenk_hash("entitiesExtentsMax"))?
  else {
    return Ok(None);
  };
  Ok(Some((minimum, maximum)))
}

fn read_position_field(
  data: &[u8],
  entries: &[MetaStructureEntry],
  name_hash: u32,
) -> Result<Option<[f64; 3]>> {
  let Some(entry) = entries.iter().find(|entry| entry.name_hash == name_hash) else {
    return Ok(None);
  };
  if entry.data_type != 0x33 {
    return Ok(None);
  }
  let start = entry.data_offset as usize;
  let Some(bytes) = data.get(start..start + 12) else {
    return Err("Truncated YMAP entity extents".into());
  };
  Ok(Some(std::array::from_fn(|axis| {
    f64::from(f32::from_le_bytes(bytes[axis * 4..axis * 4 + 4].try_into().unwrap()))
  })))
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;

  #[test]
  fn all_candidates_include_source_moved_source_and_latest_vanilla_files() {
    let root = std::env::temp_dir().join(format!("find_all_candidates_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let source = root.join("source");
    let source_cache = root.join("source-cache");
    let vanilla_cache = root.join("vanilla-cache");
    let merged = root.join("merged");
    fs::create_dir_all(source.join("resource/stream")).unwrap();
    fs::create_dir_all(source_cache.join("resources/moved/stream")).unwrap();
    fs::create_dir_all(vanilla_cache.join("latest/ymap")).unwrap();
    fs::create_dir_all(merged.join("ymap")).unwrap();
    fs::create_dir_all(merged.join("ybn")).unwrap();
    fs::write(source.join("resource/stream/live.ymap"), b"source").unwrap();
    fs::write(merged.join("ymap/merged.ymap"), b"merged").unwrap();
    fs::write(source_cache.join("resources/moved/stream/moved.ybn"), b"cached").unwrap();
    fs::write(vanilla_cache.join("latest/ymap/vanilla.ymap"), b"vanilla").unwrap();
    fs::write(
      source_cache.join("source_cache_info.json"),
      serde_json::to_vec(&serde_json::json!({
        "format_version": 3,
        "source_dir": source,
        "resources": {
          "moved": {
            "source": root.join("source/moved"),
            "files_by_format": {
              ".ybn": [{
                "path": "stream/moved.ybn",
                "file_name": "moved.ybn",
                "cached_path": "resources/moved/stream/moved.ybn"
              }]
            }
          }
        }
      }))
      .unwrap(),
    )
    .unwrap();
    fs::write(
      vanilla_cache.join("cache_info.json"),
      serde_json::to_vec(&serde_json::json!({
        "format_version": 2,
        "latest_version": "0029-mpapartment",
        "files": {
          "vanilla.ymap": {
            "version": "0029-mpapartment",
            "object": "latest/ymap/vanilla.ymap"
          }
        }
      }))
      .unwrap(),
    )
    .unwrap();

    let progress = ProgressBar::hidden();
    let search = FindEntity {
      query: super::super::EntityQuery::FileFromPosition {
        position: [0.0; 3],
      },
      merged_dir: merged,
      filter: None,
      diff_all: true,
    };
    let candidates =
      collect_candidates_from(&search, &progress, &source, &source_cache, &vanilla_cache).unwrap();

    assert!(
      candidates
        .iter()
        .any(|candidate| { candidate.name == "merged.ymap" && candidate.stage == "merged" })
    );
    assert!(
      candidates
        .iter()
        .any(|candidate| { candidate.name == "live.ymap" && candidate.stage == "source" })
    );
    assert!(candidates.iter().any(|candidate| {
      candidate.name == "moved.ybn"
        && candidate.stage == "source-cache"
        && candidate.resource.as_deref() == Some("moved")
    }));
    assert!(candidates.iter().any(|candidate| {
      candidate.name == "vanilla.ymap"
        && candidate.stage == "vanilla-cache"
        && candidate.version.as_deref() == Some("0029-mpapartment")
    }));

    fs::remove_dir_all(root).unwrap();
  }
}
