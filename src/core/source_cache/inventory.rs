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
  cached: &BTreeMap<String, FileFingerprint>,
) -> Result<BTreeMap<String, FileFingerprint>> {
  let mut files = BTreeMap::new();
  let total = resources.len();
  for (index, resource) in resources.iter().enumerate() {
    let id = resource_id(source_dir, resource)?;
    log::info!("Fingerprinting source resource {}/{}: {id}", index + 1, total);
    let before = files.len();
    files.insert(input_key(&id, ""), fingerprint(resource, None)?);
    for manifest in ["fxmanifest.lua", "__resource.lua"] {
      let path = resource.join(manifest);
      if path.is_file() {
        let key = input_key(&id, manifest);
        files.insert(key.clone(), fingerprint(&path, cached.get(&key))?);
      }
    }
    for stream_name in ["stream", "streams"] {
      let stream = resource.join(stream_name);
      if !stream.is_dir() {
        continue;
      }
      for entry in WalkDir::new(&stream).follow_links(false) {
        let entry = entry?;
        if !entry.file_type().is_file() && !entry.file_type().is_dir() {
          continue;
        }
        let relative = entry.path().strip_prefix(resource)?.to_string_lossy().replace('\\', "/");
        let key = input_key(&id, &relative);
        files.insert(key.clone(), fingerprint(entry.path(), cached.get(&key))?);
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
  cached: Option<&ResourceInventory>,
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
        let previous =
          cached.and_then(|inventory| inventory.files_by_format.get(&format)).and_then(|files| {
            files.iter().find(|file| {
              file.path == relative
                && file.sha256 == fingerprint.sha256
                && file.size == fingerprint.size
            })
          });
        let (ymap_parent_hash, metadata_error) = if let Some(previous) = previous {
          (previous.ymap_parent_hash.clone(), previous.metadata_error.clone())
        } else if format == ".ymap" {
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
          cached_path: None,
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

pub(super) fn resource_id(
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

pub(super) fn fingerprint(
  path: &Path,
  cached: Option<&FileFingerprint>,
) -> Result<FileFingerprint> {
  let metadata = fs::metadata(path)?;
  let modified = metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?;
  if metadata.is_dir() {
    let mut entries = fs::read_dir(path)?
      .map(|entry| {
        let entry = entry?;
        Ok((entry.file_name(), entry.file_type()?.is_dir()))
      })
      .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort();
    return Ok(FileFingerprint {
      sha256: format!("{:x}", Sha256::digest(serde_json::to_vec(&entries)?)),
      modified_seconds: modified.as_secs(),
      modified_nanos: modified.subsec_nanos(),
      size: entries.len() as u64,
    });
  }
  if let Some(cached) = cached
    && cached.size == metadata.len()
    && cached.modified_seconds == modified.as_secs()
    && cached.modified_nanos == modified.subsec_nanos()
  {
    return Ok(cached.clone());
  }
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn unchanged_file_metadata_reuses_hash_and_changed_metadata_rehashes() {
    let path = std::env::temp_dir().join(format!("source_fingerprint_{}", std::process::id()));
    fs::write(&path, b"source bytes").unwrap();
    let metadata = fs::metadata(&path).unwrap();
    let modified = metadata.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap();
    let cached = FileFingerprint {
      sha256: "reused-hash".into(),
      modified_seconds: modified.as_secs(),
      modified_nanos: modified.subsec_nanos(),
      size: metadata.len(),
    };

    assert_eq!(fingerprint(&path, Some(&cached)).unwrap().sha256, "reused-hash");

    let mut stale_stat = cached.clone();
    stale_stat.modified_seconds = stale_stat.modified_seconds.wrapping_add(1);
    let recalculated = fingerprint(&path, Some(&stale_stat)).unwrap();
    assert_eq!(recalculated.sha256, format!("{:x}", Sha256::digest(b"source bytes")));
    fs::remove_file(path).unwrap();
  }
}
