use std::{
  collections::BTreeMap,
  fs,
  io::BufReader,
  path::{Component, Path, PathBuf},
};

use super::{
  Result,
  types::{FileFingerprint, SourceCacheMetadata},
};

pub(super) fn cache_is_current(
  output: &Path,
  source_dir: &Path,
  vanilla_dir: &Path,
  vanilla_cache_dir: &Path,
  raw_revision: &str,
  derived_revision: &str,
  source_inputs: &BTreeMap<String, FileFingerprint>,
) -> Result<bool> {
  let metadata_path = output.join("source_cache_info.json");
  if !metadata_path.is_file() {
    return Ok(false);
  }
  let Ok(metadata) = serde_json::from_reader::<_, SourceCacheMetadata>(BufReader::new(
    fs::File::open(metadata_path)?,
  )) else {
    return Ok(false);
  };
  if metadata.format_version != 2
    || metadata.source_dir != source_dir
    || metadata.vanilla_dir != vanilla_dir
    || metadata.vanilla_cache_dir != vanilla_cache_dir
    || metadata.vanilla_manifest_sha256 != raw_revision
    || metadata.vanilla_cache_revision != derived_revision
    || metadata.source_inputs != *source_inputs
  {
    return Ok(false);
  }
  for output_file in metadata.outputs {
    let relative = Path::new(&output_file);
    if relative.components().any(|part| !matches!(part, Component::Normal(_)))
      || !output.join(relative).is_file()
    {
      return Ok(false);
    }
  }
  Ok(
    output.join("source_cache_info.json").is_file()
      && output.join("stream_conflicts.json").is_file()
      && output.join("vanilla_ymaps_to_read.json").is_file(),
  )
}

pub(super) fn prepare_output_path(
  output: &Path,
  inputs: &[&Path],
) -> Result<PathBuf> {
  let absolute =
    if output.is_absolute() { output.to_path_buf() } else { std::env::current_dir()?.join(output) };
  let parent = absolute.parent().ok_or("Source-cache output has no parent")?;
  fs::create_dir_all(parent)?;
  let parent = parent.canonicalize()?;
  let output = parent.join(absolute.file_name().ok_or("Source-cache output has no name")?);
  let current_dir = std::env::current_dir()?.canonicalize()?;
  if output == current_dir || current_dir.starts_with(&output) {
    return Err("Refusing to replace the workspace directory or one of its parents".into());
  }
  for input in inputs {
    if output == *input || output.starts_with(input) || input.starts_with(&output) {
      return Err(format!("Source-cache output overlaps input {}", input.display()).into());
    }
  }
  if fs::symlink_metadata(&output).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
    return Err(format!("Refusing to replace symlink output {}", output.display()).into());
  }
  if output.exists() && !output.is_dir() {
    return Err(format!("Source-cache output is not a directory: {}", output.display()).into());
  }
  Ok(output)
}

pub(super) fn create_staging_directory(output: &Path) -> Result<PathBuf> {
  let parent = output.parent().ok_or("Source-cache output has no parent")?;
  let name = output.file_name().ok_or("Source-cache output has no name")?.to_string_lossy();
  let staging = parent.join(format!(".{name}.staging-{}", std::process::id()));
  fs::create_dir(&staging)?;
  Ok(staging)
}

pub(super) fn publish_directory(
  staging: &Path,
  output: &Path,
) -> Result<()> {
  let backup = output.with_file_name(format!(
    ".{}.backup-{}",
    output.file_name().unwrap_or_default().to_string_lossy(),
    std::process::id()
  ));
  if backup.exists() {
    return Err(format!("Source-cache backup already exists: {}", backup.display()).into());
  }
  let existed = output.exists();
  if existed {
    fs::rename(output, &backup)?;
  }
  if let Err(error) = fs::rename(staging, output) {
    if existed {
      fs::rename(&backup, output)?;
    }
    return Err(error.into());
  }
  if existed && let Err(error) = fs::remove_dir_all(backup) {
    log::warn!("Could not remove source-cache backup: {error}");
  }
  Ok(())
}
