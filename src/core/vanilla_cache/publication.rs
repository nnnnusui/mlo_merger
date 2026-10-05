//! Scratch cleanup, completed-cache publication and rollback.

use std::fs;
use std::io::BufReader;
use std::path::{Component, Path, PathBuf};

use super::{
  Result,
  manifest::{CacheVersion, VanillaCacheManifest},
};

pub(super) struct Staging(pub(super) PathBuf);

impl Drop for Staging {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.0);
  }
}

pub(super) fn reset_output_directory(
  output: &Path,
  game_dir: &Path,
) -> Result<PathBuf> {
  let output = normalize_absolute(output)?;
  let game_dir = game_dir.canonicalize()?;
  let current_dir = std::env::current_dir()?.canonicalize()?;
  validate_output_location(&output, &game_dir, &current_dir)?;
  let parent = output.parent().ok_or("Cache output has no parent directory")?;
  let name = output.file_name().ok_or("Cache output has no directory name")?;
  fs::create_dir_all(parent)?;
  let parent = parent.canonicalize()?;
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
  Ok(output.canonicalize()?)
}

fn normalize_absolute(path: &Path) -> Result<PathBuf> {
  let path =
    if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir()?.join(path) };
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

/// Reject unknown output directories and retain backups until root metadata
/// is installed, restoring the previous cache if publication fails.
pub(super) fn publish_cache(
  build: &Path,
  output: &Path,
  manifest: &VanillaCacheManifest,
) -> Result<()> {
  for version in &manifest.versions {
    let destination = output.join(&version.id);
    if destination.exists() {
      let existing: CacheVersion = serde_json::from_reader(BufReader::new(fs::File::open(
        destination.join("version_info.json"),
      )?))?;
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
  let result = (|| -> Result<()> {
    for version in &manifest.versions {
      let destination = output.join(&version.id);
      let existed = destination.exists();
      if existed {
        fs::rename(&destination, backup.join(&version.id))?;
      }
      published.push((version.id.clone(), existed));
      fs::rename(build.join(&version.id), destination)?;
    }
    fs::rename(build.join("cache_info.json"), output.join("cache_info.json"))?;
    Ok(())
  })();
  if result.is_err() {
    for (id, existed) in published.into_iter().rev() {
      let destination = output.join(&id);
      if destination.exists() {
        fs::rename(&destination, build.join(&id))?;
      }
      if existed {
        fs::rename(backup.join(&id), destination)?;
      }
    }
  }
  if let Err(error) = fs::remove_dir_all(&backup) {
    log::warn!("Could not clean cache backup {}: {error}", backup.display());
  }
  result
}

pub(super) fn preserve_failed_logs(
  build: &Path,
  output: &Path,
) -> Result<PathBuf> {
  let failure = output.join(format!("failed-{}", chrono::Local::now().format("%Y%m%d-%H%M%S-%f")));
  fs::create_dir(&failure)?;
  for entry in fs::read_dir(build)? {
    let entry = entry?;
    let log = entry.path().join("create_cache.log");
    if log.is_file() {
      let directory = failure.join(entry.file_name());
      fs::create_dir(&directory)?;
      fs::copy(log, directory.join("create_cache.log"))?;
    }
  }
  Ok(failure)
}
