//! Safe file operations in the deployment directory.

use super::Result;
use crate::platform::fs;
use crate::platform::fs::WalkDir;
use crate::platform::process;
use std::{
  io::Write,
  path::{Component, Path, PathBuf},
};

/// Removes empty directories below `stream/`, without following links.
pub(super) fn remove_empty_stream_directories(output: &Path) -> Result<()> {
  let stream = destination(output, "stream")?;
  if !fs::is_dir(&stream) {
    return Ok(());
  }
  for entry in WalkDir::new(&stream).follow_links(false).contents_first(true) {
    let entry = entry?;
    if !entry.file_type().is_dir() {
      continue;
    }
    match fs::remove_dir(entry.path()) {
      Ok(()) => {}
      Err(error)
        if matches!(
          error.kind(),
          std::io::ErrorKind::DirectoryNotEmpty | std::io::ErrorKind::NotFound
        ) => {}
      Err(error) => return Err(error.into()),
    }
  }
  Ok(())
}

/// Copies `source` to `target` through a temporary file, keeping its modification time.
pub(super) fn copy_file(
  source: &Path,
  target: &Path,
) -> Result<()> {
  let parent = target.parent().ok_or("Deployment file has no parent")?;
  fs::create_dir_all(parent)?;
  let temporary = parent.join(format!(
    ".{}.deploy-{}.tmp",
    target.file_name().ok_or("Deployment file has no name")?.to_string_lossy(),
    process::id()
  ));
  let mut writer = fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
  let result = (|| -> Result<()> {
    let mut reader = fs::File::open(source)?;
    std::io::copy(&mut reader, &mut writer)?;
    writer.flush()?;
    writer.set_times(fs::FileTimes::new().set_modified(reader.metadata()?.modified()?))?;
    drop(writer);
    fs::rename(&temporary, target)?;
    Ok(())
  })();
  if result.is_err() {
    let _ = fs::remove_file(temporary);
  }
  result
}

/// Creates the output directory, rejecting one that overlaps the working directory or `inputs`
/// or is a link.
pub(super) fn prepare_output(
  path: &Path,
  inputs: &[&Path],
) -> Result<PathBuf> {
  let absolute =
    if path.is_absolute() { path.to_path_buf() } else { process::current_dir()?.join(path) };
  let parent = absolute.parent().ok_or("Deployment output has no parent")?;
  fs::create_dir_all(parent)?;
  let output =
    fs::canonicalize(parent)?.join(absolute.file_name().ok_or("Deployment output has no name")?);
  let current = fs::canonicalize(&process::current_dir()?)?;
  if current.starts_with(&output)
    || inputs.iter().any(|input| output.starts_with(input) || input.starts_with(&output))
  {
    return Err("Deployment output overlaps workspace or inputs".into());
  }
  if fs::symlink_metadata(&output).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
    return Err("Symlink deployment output is not supported".into());
  }
  fs::create_dir_all(&output)?;
  Ok(output)
}

/// `relative` below `root`, rejecting components that leave it or pass through a link.
pub(super) fn destination(
  root: &Path,
  relative: &str,
) -> Result<PathBuf> {
  let mut target = root.to_path_buf();
  for component in Path::new(relative).components() {
    if !matches!(component, Component::Normal(_)) {
      return Err("Unsafe deployment cache path".into());
    }
    target.push(component);
    if fs::symlink_metadata(&target).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
      return Err(format!("Symlink deployment path is not supported: {}", target.display()).into());
    }
  }
  Ok(target)
}
