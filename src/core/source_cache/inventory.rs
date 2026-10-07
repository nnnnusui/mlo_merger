use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use super::{
  Result,
  types::{DerivedVanillaFile, FileFingerprint, ResourceInventory, SourceFile, VanillaMatch},
};
use crate::core::{common::function::get_resource_directories, vanilla_cache::ymap_parent_hash};

pub(super) fn discover_resources(source_dir: &Path) -> Result<Vec<PathBuf>> {
  let resources = get_resource_directories(source_dir)?;
  if resources.is_empty() {
    return Err(format!("No FiveM resources found in {}", source_dir.display()).into());
  }
  Ok(resources)
}

pub(super) fn source_fingerprints(
  source_dir: &Path,
  resources: &[PathBuf],
) -> Result<BTreeMap<String, FileFingerprint>> {
  let mut files = BTreeMap::new();
  let total = resources.len();
  for (index, resource) in resources.iter().enumerate() {
    let id = resource_id(source_dir, resource)?;
    log::info!("Fingerprinting source resource {}/{}: {id}", index + 1, total);
    let before = files.len();
    for manifest in ["fxmanifest.lua", "__resource.lua"] {
      let path = resource.join(manifest);
      if path.is_file() {
        files.insert(input_key(&id, manifest), fingerprint(&path)?);
      }
    }
    for stream_name in ["stream", "streams"] {
      let stream = resource.join(stream_name);
      if !stream.is_dir() {
        continue;
      }
      for entry in WalkDir::new(&stream).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() {
          continue;
        }
        let relative = entry.path().strip_prefix(resource)?.to_string_lossy().replace('\\', "/");
        files.insert(input_key(&id, &relative), fingerprint(entry.path())?);
      }
    }
    log::info!("Fingerprint complete for {id}: {} files", files.len() - before);
  }
  Ok(files)
}

pub(super) fn source_inventory(
  source_dir: &Path,
  resources: &[PathBuf],
  inputs: &BTreeMap<String, FileFingerprint>,
  vanilla_files: &BTreeMap<String, DerivedVanillaFile>,
) -> Result<BTreeMap<String, ResourceInventory>> {
  let mut inventories = BTreeMap::new();
  let total = resources.len();
  for (index, resource) in resources.iter().enumerate() {
    let id = resource_id(source_dir, resource)?;
    log::info!("Inventorying source resource {}/{}: {id}", index + 1, total);
    let mut files_by_format: BTreeMap<String, Vec<SourceFile>> = BTreeMap::new();
    for stream_name in ["stream", "streams"] {
      let stream = resource.join(stream_name);
      if !stream.is_dir() {
        continue;
      }
      for entry in WalkDir::new(&stream).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() {
          continue;
        }
        let path = entry.path();
        let relative = path.strip_prefix(resource)?.to_string_lossy().replace('\\', "/");
        let key = input_key(&id, &relative);
        let fingerprint = inputs.get(&key).ok_or("Source fingerprint is missing")?;
        let file_name = path
          .file_name()
          .and_then(|name| name.to_str())
          .ok_or("Source filename is not UTF-8")?
          .to_ascii_lowercase();
        let format = path
          .extension()
          .and_then(|extension| extension.to_str())
          .map(|extension| format!(".{}", extension.to_ascii_lowercase()))
          .unwrap_or_else(|| "[no extension]".into());
        let vanilla = if matches!(format.as_str(), ".ymap" | ".ybn") {
          vanilla_files.get(&file_name).map(|file| VanillaMatch {
            version: file.version.clone(),
            sha256: file.sha256.clone(),
            content_matches: file.sha256 == fingerprint.sha256,
          })
        } else {
          None
        };
        let (ymap_parent_hash, metadata_error) = if format == ".ymap" {
          match ymap_parent_hash(path) {
            Ok(hash) => (Some(format!("{hash:08X}")), None),
            Err(error) => {
              log::warn!("Could not read YMAP parent reference {}: {error}", path.display());
              (None, Some(error.to_string()))
            }
          }
        } else {
          (None, None)
        };
        files_by_format.entry(format).or_default().push(SourceFile {
          path: relative,
          file_name,
          sha256: fingerprint.sha256.clone(),
          size: fingerprint.size,
          modified_seconds: fingerprint.modified_seconds,
          modified_nanos: fingerprint.modified_nanos,
          vanilla,
          ymap_parent_hash,
          metadata_error,
        });
      }
    }
    let file_count: usize = files_by_format.values().map(Vec::len).sum();
    for files in files_by_format.values_mut() {
      files.sort_by(|left, right| left.path.cmp(&right.path));
    }
    inventories.insert(
      id.clone(),
      ResourceInventory {
        source: resource.clone(),
        files_by_format,
      },
    );
    log::info!("Inventory complete for {id}: {file_count} stream files");
  }
  Ok(inventories)
}

fn resource_id(
  source_dir: &Path,
  resource: &Path,
) -> Result<String> {
  let relative = resource.strip_prefix(source_dir)?.to_string_lossy().replace('\\', "/");
  if relative.is_empty() {
    Ok(resource.file_name().ok_or("Resource directory has no name")?.to_string_lossy().into_owned())
  } else {
    Ok(relative)
  }
}

fn input_key(
  resource_id: &str,
  relative: &str,
) -> String {
  format!("{resource_id}/{relative}")
}

fn fingerprint(path: &Path) -> Result<FileFingerprint> {
  let bytes = fs::read(path)?;
  let metadata = fs::metadata(path)?;
  let modified = metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?;
  Ok(FileFingerprint {
    sha256: format!("{:x}", Sha256::digest(&bytes)),
    modified_seconds: modified.as_secs(),
    modified_nanos: modified.subsec_nanos(),
    size: metadata.len(),
  })
}
