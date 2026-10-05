//! Scratch cleanup, completed-cache publication and rollback.

use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use super::{
  Result,
  manifest::{CacheVersion, GtavCacheManifest},
};

pub(super) struct Staging(pub(super) PathBuf);

impl Drop for Staging {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.0);
  }
}

pub(super) fn publish_cache(
  build: &Path,
  output: &Path,
  manifest: &GtavCacheManifest,
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
