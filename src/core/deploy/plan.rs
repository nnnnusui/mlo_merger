//! Which files a deployment manages and where each comes from.

use super::Result;
use crate::core::source_cache::CachedFile;
use crate::fs;
use crate::fs::WalkDir;
use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Path, PathBuf},
};

/// The managed deployment files, each with its source and that source's path relative to its
/// store: every merged YMAP/YBN, and each file the source cache moved out of its resource whose
/// name no merged file takes.
pub(super) fn plan(
  merged: &Path,
  cache: &Path,
  cached: &[CachedFile],
) -> Result<BTreeMap<String, (PathBuf, PathBuf)>> {
  let mut files = BTreeMap::new();
  let mut merged_names = BTreeSet::new();
  for entry in WalkDir::new(merged).follow_links(false) {
    let entry = entry?;
    if entry.file_type().is_symlink() {
      return Err("Symlink merged input is not supported".into());
    }
    if !entry.file_type().is_file() {
      continue;
    }
    let path = entry.path();
    let extension = extension(path);
    if !matches!(extension.as_str(), "ymap" | "ybn") {
      continue;
    }
    let name =
      path.file_name().and_then(|name| name.to_str()).ok_or("Merged filename is not UTF-8")?;
    if !merged_names.insert(name.to_ascii_lowercase()) {
      return Err(format!("Duplicate merged stream basename: {name}").into());
    }
    files.insert(
      format!("stream/{extension}/merged/{name}"),
      (path.to_path_buf(), path.strip_prefix(merged)?.to_path_buf()),
    );
  }
  for file in cached {
    let path = cache.join(&file.cached_path);
    if fs::symlink_metadata(&path)?.file_type().is_symlink() {
      return Err("Symlink source-cache input is not supported".into());
    }
    let name =
      path.file_name().and_then(|name| name.to_str()).ok_or("Cached filename is not UTF-8")?;
    if merged_names.contains(&name.to_ascii_lowercase()) {
      continue;
    }
    let relative = file.cached_path.strip_prefix("resources/").ok_or("Unsafe source-cache path")?;
    files.insert(
      format!("stream/{}/clone/{relative}", extension(&path)),
      (path, PathBuf::from(&file.cached_path)),
    );
  }
  Ok(files)
}

fn extension(path: &Path) -> String {
  path
    .extension()
    .and_then(|value| value.to_str())
    .map(|value| value.to_ascii_lowercase())
    .unwrap_or_else(|| "no_extension".into())
}
