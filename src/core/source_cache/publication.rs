use super::Result;
#[cfg(test)]
use super::types::{FileFingerprint, SourceCacheMetadata};
#[cfg(test)]
use std::{collections::BTreeMap, io::BufReader};
use std::{
  fs,
  path::{Path, PathBuf},
};

#[cfg(test)]
pub(super) fn same_source_content(
  cached: &BTreeMap<String, FileFingerprint>,
  current: &BTreeMap<String, FileFingerprint>,
) -> bool {
  cached.len() == current.len()
    && cached.iter().all(|(path, cached_fingerprint)| {
      current.get(path).is_some_and(|current_fingerprint| {
        cached_fingerprint.size == current_fingerprint.size
          && cached_fingerprint.sha256 == current_fingerprint.sha256
      })
    })
}

#[cfg(test)]
pub(super) fn refresh_source_timestamps(
  output: &Path,
  source_inputs: &BTreeMap<String, FileFingerprint>,
) -> Result<()> {
  let metadata_path = output.join("source_cache_info.json");
  let mut metadata: SourceCacheMetadata =
    serde_json::from_reader(BufReader::new(fs::File::open(&metadata_path)?))?;
  let mut changed = false;
  for (path, cached) in &mut metadata.source_inputs {
    let current =
      source_inputs.get(path).ok_or("Source input disappeared during fingerprinting")?;
    if cached.sha256 != current.sha256 || cached.size != current.size {
      return Err(format!("Source content changed while refreshing timestamps: {path}").into());
    }
    if cached.modified_seconds != current.modified_seconds
      || cached.modified_nanos != current.modified_nanos
    {
      cached.modified_seconds = current.modified_seconds;
      cached.modified_nanos = current.modified_nanos;
      changed = true;
    }
  }
  for (resource_id, resource) in &mut metadata.resources {
    for files in resource.files_by_format.values_mut() {
      for source in files {
        let key = format!("{resource_id}/{}", source.path);
        let current = source_inputs.get(&key).ok_or("Source stream input disappeared")?;
        if source.sha256 != current.sha256 || source.size != current.size {
          return Err(format!("Source content changed while refreshing timestamps: {key}").into());
        }
        if source.modified_seconds != current.modified_seconds
          || source.modified_nanos != current.modified_nanos
        {
          source.modified_seconds = current.modified_seconds;
          source.modified_nanos = current.modified_nanos;
          changed = true;
        }
      }
    }
  }
  if !changed {
    return Ok(());
  }
  use std::io::Write;
  let mut writer = std::io::BufWriter::new(fs::File::create(metadata_path)?);
  serde_json::to_writer_pretty(&mut writer, &metadata)?;
  writer.flush()?;
  Ok(())
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn source_cache_content_freshness_ignores_timestamp_only_changes() {
    let fingerprint = FileFingerprint {
      sha256: "same-content".into(),
      modified_seconds: 10,
      modified_nanos: 20,
      size: 30,
    };
    let cached = BTreeMap::from([("resource/stream/map.ymap".into(), fingerprint.clone())]);
    let mut touched = fingerprint;
    touched.modified_seconds += 1;
    touched.modified_nanos += 1;
    let current = BTreeMap::from([("resource/stream/map.ymap".into(), touched)]);

    assert!(same_source_content(&cached, &current));
  }
}
