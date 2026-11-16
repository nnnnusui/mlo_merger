//! Scratch cleanup, completed-cache publication and rollback.

use crate::platform::fs;
use crate::platform::process;
use std::path::{Component, Path, PathBuf};

use super::{
  Result,
  manifest::{CacheVersion, VanillaCacheManifest},
};

pub(crate) struct Staging(pub(crate) PathBuf);

impl Drop for Staging {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.0);
  }
}

pub(crate) fn reset_output_directory(
  output: &Path,
  game_dir: &Path,
) -> Result<PathBuf> {
  let output = normalize_absolute(output)?;
  let game_dir = fs::canonicalize(game_dir)?;
  let current_dir = fs::canonicalize(&process::current_dir()?)?;
  validate_output_location(&output, &game_dir, &current_dir)?;
  let parent = output.parent().ok_or("Cache output has no parent directory")?;
  let name = output.file_name().ok_or("Cache output has no directory name")?;
  fs::create_dir_all(parent)?;
  let parent = fs::canonicalize(parent)?;
  let output = parent.join(name);
  validate_output_location(&output, &game_dir, &current_dir)?;
  if let Ok(metadata) = fs::symlink_metadata(&output) {
    if metadata.file_type().is_symlink() {
      return Err(format!("Refusing to remove symlink cache output {}", output.display()).into());
    }
    if !metadata.is_dir() {
      return Err(format!("Cache output is not a directory: {}", output.display()).into());
    }
    fs::remove_dir_all(&output)?;
  }
  fs::create_dir_all(&output)?;
  Ok(fs::canonicalize(&output)?)
}

fn normalize_absolute(path: &Path) -> Result<PathBuf> {
  let path =
    if path.is_absolute() { path.to_path_buf() } else { process::current_dir()?.join(path) };
  let mut normalized = PathBuf::new();
  for component in path.components() {
    match component {
      Component::CurDir => {}
      Component::ParentDir => {
        if normalized.file_name().is_some() {
          normalized.pop();
        }
      }
      component => normalized.push(component.as_os_str()),
    }
  }
  Ok(normalized)
}

fn validate_output_location(
  output: &Path,
  game_dir: &Path,
  current_dir: &Path,
) -> Result<()> {
  if output == game_dir || output.starts_with(game_dir) || game_dir.starts_with(output) {
    return Err("Cache output must not overlap the installed game directory".into());
  }
  if output == current_dir || current_dir.starts_with(output) {
    return Err("Refusing to clear the workspace directory or one of its parents".into());
  }
  Ok(())
}

const LATEST_VERSION_PREFIX: &str = "latest_version-";

/// Names the marker for the final listed stage, whether or not it wrote files.
pub(crate) fn latest_version_marker(manifest: &VanillaCacheManifest) -> Option<String> {
  manifest.versions.last().map(|version| format!("{LATEST_VERSION_PREFIX}{}", version.id))
}

/// Reject unknown output directories and retain backups until root metadata
/// is installed, restoring the previous cache if publication fails.
pub(crate) fn publish_cache(
  build: &Path,
  output: &Path,
  manifest: &VanillaCacheManifest,
  extra_metadata_files: &[&str],
  obsolete_files: &[&str],
) -> Result<()> {
  let marker = latest_version_marker(manifest).ok_or("Vanilla archive has no stages")?;
  fs::write(build.join(&marker), [])?;
  let mut metadata_files: Vec<_> =
    extra_metadata_files.iter().map(|name| (*name).to_owned()).collect();
  metadata_files.push("cache_info.json".to_owned());
  metadata_files.push(marker.clone());
  for entry in fs::read_dir(output)? {
    let entry = entry?;
    let name = entry.file_name().to_string_lossy().into_owned();
    if name.starts_with(LATEST_VERSION_PREFIX)
      && name != marker
      && entry.file_type()?.is_file()
      && entry.metadata()?.len() == 0
    {
      metadata_files.push(name);
    }
  }
  for name in &metadata_files {
    let metadata_path = output.join(name);
    if fs::exists(&metadata_path).unwrap_or(false) && !fs::is_file(&metadata_path) {
      return Err(
        format!("Refusing to replace non-file cache metadata {}", metadata_path.display()).into(),
      );
    }
  }
  for version in &manifest.versions {
    let destination = output.join(&version.id);
    if fs::exists(&destination).unwrap_or(false) {
      let existing: CacheVersion =
        crate::common::json::read_json(destination.join("version_info.json"))?;
      if existing.id != version.id {
        return Err(
          format!("Refusing to replace unrecognized cache directory {}", destination.display())
            .into(),
        );
      }
    }
  }
  let backup = output.join(format!(".backup-{}", chrono::Local::now().format("%Y%m%d-%H%M%S-%f")));
  fs::create_dir(&backup)?;
  let mut published = Vec::new();
  let mut published_metadata = Vec::new();
  let result = (|| -> Result<()> {
    for version in &manifest.versions {
      let destination = output.join(&version.id);
      let existed = fs::exists(&destination).unwrap_or(false);
      if existed {
        fs::rename(&destination, backup.join(&version.id))?;
      }
      published.push((version.id.clone(), existed));
      if !version.changes.is_empty() {
        fs::rename(build.join(&version.id), destination)?;
      }
    }
    for name in &metadata_files {
      let source = build.join(name);
      let retired_marker = name.starts_with(LATEST_VERSION_PREFIX) && *name != marker;
      if !retired_marker && !fs::is_file(&source) {
        continue;
      }
      let destination = output.join(name);
      let existed = fs::exists(&destination).unwrap_or(false);
      if existed {
        fs::rename(&destination, backup.join(name))?;
      }
      published_metadata.push((name.clone(), existed));
      if !retired_marker {
        fs::rename(source, destination)?;
      }
    }
    Ok(())
  })();
  if result.is_err() {
    for (name, existed) in published_metadata.into_iter().rev() {
      let destination = output.join(&name);
      if fs::exists(&destination).unwrap_or(false) {
        fs::rename(&destination, build.join(&name))?;
      }
      if existed {
        fs::rename(backup.join(name), destination)?;
      }
    }
    for (id, existed) in published.into_iter().rev() {
      let destination = output.join(&id);
      if fs::exists(&destination).unwrap_or(false) {
        fs::rename(&destination, build.join(&id))?;
      }
      if existed {
        fs::rename(backup.join(&id), destination)?;
      }
    }
  }
  if let Err(error) = fs::remove_dir_all(&backup) {
    tracing::warn!("Could not clean cache backup {}: {error}", backup.display());
  }
  if result.is_ok() {
    for name in obsolete_files {
      let path = output.join(name);
      if fs::is_file(&path) {
        fs::remove_file(&path)?;
      }
    }
  }
  result
}
