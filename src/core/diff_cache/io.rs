//! Cache-relative path validation, file hashing and scratch cleanup.

use super::Result;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub(super) fn cache_path(
  root: &Path,
  relative: &str,
) -> Result<PathBuf> {
  let path = Path::new(relative);
  if relative.is_empty()
    || relative.contains(['\\', ':'])
    || path.components().any(|component| !matches!(component, Component::Normal(_)))
  {
    return Err(format!("Invalid cache-relative path: {relative}").into());
  }
  Ok(root.join(path))
}

pub(super) fn content_hash(path: &Path) -> Result<String> {
  Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

#[cfg(test)]
pub(super) struct Scratch(pub(super) PathBuf);
#[cfg(test)]
impl Drop for Scratch {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.0);
  }
}
