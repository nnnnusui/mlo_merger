//! Live stream discovery for on-demand exports; overwrite uses recorded inventory instead.

use super::{
  Result,
  streams::{StreamFile as ResourceFile, StreamInventory as Inventory},
};
use crate::common::path::{reject_symlinks, safe_relative};
use crate::core::{common::function::get_resource_directories, format::ymap::diff::reference_hash};
use serde::Deserialize;
use std::{
  collections::BTreeMap,
  fs,
  io::BufReader,
  path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Deserialize)]
struct Cache {
  format_version: u32,
  source_dir: PathBuf,
  resources: BTreeMap<String, CachedResource>,
}
#[derive(Deserialize)]
struct CachedResource {
  source: PathBuf,
  files_by_format: BTreeMap<String, Vec<CachedFile>>,
}
#[derive(Deserialize)]
struct CachedFile {
  path: String,
  cached_path: Option<String>,
  ymap_parent_hash: Option<String>,
  size: u64,
  modified_seconds: u64,
  modified_nanos: u32,
}

#[cfg(test)]
pub(crate) fn discover_inventory(
  source: &Path,
  cache: &Path,
) -> Result<Inventory> {
  inventory_selected(source, cache, None)
}

pub(crate) fn inventory_for(
  source: &Path,
  cache: &Path,
  resource: &str,
) -> Result<Inventory> {
  let exact = inventory_selected(source, cache, Some(resource))?;
  if !exact.is_empty() || resource.contains('/') {
    return Ok(exact);
  }
  let root = source.canonicalize()?;
  let matches = get_resource_directories(&root)?
    .into_iter()
    .filter(|path| path.file_name().and_then(|name| name.to_str()) == Some(resource))
    .collect::<Vec<_>>();
  if matches.len() > 1 {
    return Err(
      format!("Ambiguous light resource name {resource}; specify its source-relative group path")
        .into(),
    );
  }
  let Some(path) = matches.first() else {
    return Ok(exact);
  };
  let id = path.strip_prefix(&root)?.to_string_lossy().replace('\\', "/");
  inventory_selected(source, cache, Some(&id))
}

fn inventory_selected(
  source: &Path,
  cache: &Path,
  selection: Option<&str>,
) -> Result<Inventory> {
  let mut inventory = Inventory::new();
  let root = source.canonicalize()?;
  for resource in get_resource_directories(&root)? {
    let id = resource.strip_prefix(&root)?.to_string_lossy().replace('\\', "/");
    let id = if id.is_empty() {
      resource.file_name().and_then(|name| name.to_str()).ok_or("Invalid resource name")?.to_owned()
    } else {
      id
    };
    safe_relative(&id)?;
    if selection.is_some_and(|selection| selection != id) {
      continue;
    }
    let files = inventory.entry(id).or_default();
    for stream in ["stream", "streams"] {
      let directory = resource.join(stream);
      if !directory.is_dir() {
        continue;
      }
      reject_symlinks(&directory)?;
      for entry in WalkDir::new(&directory).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_symlink() {
          return Err("Symlink in resource light inventory".into());
        }
        if !entry.file_type().is_file() || !is_ymap(entry.path()) {
          continue;
        }
        let relative = entry.path().strip_prefix(&directory)?.to_string_lossy().replace('\\', "/");
        safe_relative(&relative)?;
        if files
          .insert(
            relative,
            ResourceFile {
              path: entry.path().to_path_buf(),
              original: entry.path().to_path_buf(),
              parent_hint: None,
            },
          )
          .is_some()
        {
          return Err("Ambiguous stream/streams YMAP path".into());
        }
      }
    }
  }
  let metadata = cache.join("source_cache_info.json");
  if metadata.is_file() {
    let metadata: Cache = serde_json::from_reader(BufReader::new(fs::File::open(metadata)?))?;
    if !matches!(metadata.format_version, 2 | 3) {
      return Err("Unsupported source cache schema".into());
    }
    if metadata.source_dir.canonicalize()? != root {
      return Err("Light source cache belongs to another source root".into());
    }
    for (id, resource) in metadata.resources {
      safe_relative(&id)?;
      let Some(files) = inventory.get_mut(&id) else {
        continue;
      };
      for file in resource.files_by_format.get(".ymap").into_iter().flatten() {
        safe_relative(&file.path)?;
        let relative = stream_relative(&file.path)?;
        if let Some(cached) = &file.cached_path {
          safe_relative(cached)?;
          if cached.starts_with("_old/") {
            return Err("Archived light dependency".into());
          }
          let path = cache.join(cached);
          reject_symlinks(&path)?;
          if files.contains_key(&relative) {
            return Err(format!("Both live and cached light input: {id}/{relative}").into());
          }
          if !path.is_file() {
            return Err(format!("Missing light input {}", path.display()).into());
          }
          files.insert(
            relative.clone(),
            ResourceFile {
              path,
              original: resource.source.join(&file.path),
              parent_hint: None,
            },
          );
        }
        if let Some(input) = files.get_mut(&relative) {
          let stats = fs::metadata(&input.path)?;
          let time = stats.modified()?.duration_since(std::time::UNIX_EPOCH)?;
          if stats.len() == file.size
            && time.as_secs() == file.modified_seconds
            && time.subsec_nanos() == file.modified_nanos
          {
            input.parent_hint =
              file.ymap_parent_hash.as_ref().and_then(|hash| u32::from_str_radix(hash, 16).ok());
          }
        }
      }
    }
  }
  for files in inventory.values() {
    let mut names = BTreeMap::new();
    for path in files.keys() {
      let hash = file_hash(path)?;
      if names.insert(hash, path).is_some() {
        return Err("Ambiguous YMAP name/hash within resource".into());
      }
    }
  }
  Ok(inventory)
}

fn file_hash(path: &str) -> Result<u32> {
  Ok(reference_hash(
    Path::new(path).file_stem().and_then(|name| name.to_str()).ok_or("Invalid YMAP name")?,
  ))
}

fn is_ymap(path: &Path) -> bool {
  path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("ymap"))
}

fn stream_relative(path: &str) -> Result<String> {
  for prefix in ["stream/", "streams/"] {
    if let Some(relative) = path.strip_prefix(prefix) {
      safe_relative(relative)?;
      return Ok(relative.to_owned());
    }
  }
  Err(format!("Light input is outside stream: {path}").into())
}
