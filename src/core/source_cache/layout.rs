//! Migrates cached stream paths while preserving original resource paths.

use super::{Result, types::ResourceInventory};
use crate::common::changes::StoreChanges;
use crate::common::{filesystem::move_file, path::ensure_safe_path};
use crate::fs;
use std::{collections::BTreeSet, path::Path};

pub(super) fn stream_relative_path(path: &str) -> Result<&str> {
  path
    .strip_prefix("stream/")
    .or_else(|| path.strip_prefix("streams/"))
    .filter(|relative| !relative.is_empty())
    .ok_or_else(|| format!("Source file is not under a stream directory: {path}").into())
}

/// Preserves original stream paths while migrating recorded cache paths without replacing files,
/// recording each moved file in `changes`.
pub(super) fn normalize_cached_layout(
  output: &Path,
  name: &str,
  inventory: &mut ResourceInventory,
  changes: &mut StoreChanges,
) -> Result<()> {
  let mut moves = Vec::new();
  for file in inventory.files_by_format.values().flatten() {
    let Some(old) = &file.cached_path else {
      continue;
    };
    let new = format!("resources/{name}/{}", stream_relative_path(&file.path)?);
    if old != &new {
      ensure_safe_path(output, &output.join(old))?;
      ensure_safe_path(output, &output.join(&new))?;
      moves.push((old.clone(), new, file.sha256.clone()));
    }
  }
  let old_paths = moves.iter().map(|(old, ..)| old.as_str()).collect::<BTreeSet<_>>();
  for (old, new, _) in &moves {
    if fs::is_file(output.join(old))
      && fs::exists(output.join(new)).unwrap_or(false)
      && !old_paths.contains(new.as_str())
    {
      return Err(
        format!("Cache layout migration would overwrite {}", output.join(new).display()).into(),
      );
    }
  }
  moves.sort_by_key(|(old, ..)| Path::new(old).components().count());
  let mut directories = BTreeSet::new();
  for (old, new, sha256) in &moves {
    let source = output.join(old);
    let destination = output.join(new);
    if fs::is_file(&source) {
      if fs::exists(&destination).unwrap_or(false) {
        return Err(
          format!("Cache layout migration would overwrite {}", destination.display()).into(),
        );
      }
      fs::create_dir_all(destination.parent().ok_or("Cache file has no parent")?)?;
      move_file(&source, &destination)?;
      changes.record(old, Some(sha256), None);
      changes.record(new, None, Some(sha256));
    }
    for parent in source.ancestors().skip(1).take_while(|parent| {
      parent.starts_with(output)
        && *parent != output
        && *parent != output.join("resources")
        && *parent != output.join("resources").join(name)
    }) {
      directories.insert(parent.to_path_buf());
    }
  }
  let mut directories = directories.into_iter().collect::<Vec<_>>();
  directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
  for directory in directories {
    if let Err(error) = fs::remove_dir(&directory)
      && !matches!(
        error.kind(),
        std::io::ErrorKind::DirectoryNotEmpty | std::io::ErrorKind::NotFound
      )
    {
      return Err(error.into());
    }
  }
  for file in inventory.files_by_format.values_mut().flatten() {
    if file.cached_path.is_some() {
      file.cached_path = Some(format!("resources/{name}/{}", stream_relative_path(&file.path)?));
    }
  }
  Ok(())
}
