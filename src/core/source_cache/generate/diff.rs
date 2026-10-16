//! Writes each source YMAP's differences from vanilla as `diff/{resource}/{path}.diff.json`.

use super::Result;
use crate::{
  common::{json::write_json, path::ensure_safe_path},
  core::{
    conversion::load_hash_names_or_empty,
    format::ymap::compare::{StreamFileRef, YmapComparisonContext},
    source_cache::{layout::stream_relative_path, types::SourceCacheMetadata},
  },
};
use std::{fs, io::BufReader, path::Path};

/// Regenerates the `diff` directory for every source YMAP that overrides a vanilla YMAP.
///
/// Reads the published `source_cache_info.json`, so it runs after publication. Returns the
/// number of diff files written; YMAPs whose diff cannot be built are logged and skipped.
pub(super) fn write_ymap_diffs(
  output: &Path,
  vanilla_dir: &Path,
  vanilla_cache_dir: &Path,
) -> Result<usize> {
  let metadata: SourceCacheMetadata = serde_json::from_reader(BufReader::new(fs::File::open(
    output.join("source_cache_info.json"),
  )?))?;
  let hash_names = load_hash_names_or_empty(&vanilla_dir.join("hash_names.json"));
  let mut context = YmapComparisonContext::new(output, vanilla_cache_dir, hash_names)?;

  let staging = output.join("diff.tmp");
  let target = output.join("diff");
  ensure_safe_path(output, &staging)?;
  ensure_safe_path(output, &target)?;
  remove_dir_if_exists(&staging)?;

  let mut written = 0;
  for resource in metadata.resources.values() {
    let resource_name =
      resource.source.file_name().ok_or("Resource has no name")?.to_string_lossy().into_owned();
    for file in resource.files_by_format.get(".ymap").into_iter().flatten() {
      if file.vanilla.is_none() {
        continue;
      }
      let relative = stream_relative_path(&file.path)?;
      let stream_file = StreamFileRef {
        resource_name: Some(resource_name.clone()),
        vanilla_version: metadata.latest_vanilla_version.clone(),
        file_name: file.file_name.clone(),
      };
      let diff = match context.get_diff(&stream_file) {
        Ok(diff) => diff,
        Err(error) => {
          tracing::warn!("Skipping YMAP diff for {resource_name}/{relative}: {error}");
          continue;
        }
      };
      let path = staging.join(&resource_name).join(format!("{relative}.diff.json"));
      ensure_safe_path(&staging, &path)?;
      fs::create_dir_all(path.parent().ok_or("Diff path has no parent")?)?;
      write_json(&path, &diff)?;
      written += 1;
    }
  }

  fs::create_dir_all(&staging)?;
  remove_dir_if_exists(&target)?;
  fs::rename(&staging, &target)?;
  tracing::info!("Wrote {written} YMAP diffs to {}", target.display());
  Ok(written)
}

fn remove_dir_if_exists(path: &Path) -> Result<()> {
  match fs::remove_dir_all(path) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  #[ignore = "requires local asset/source-cache and asset/vanilla-cache"]
  fn writes_ymap_diffs_for_local_source_cache() {
    let asset = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset");
    let source_cache = asset.join("source-cache");
    let output = std::env::temp_dir().join(format!("ymap_diffs_{}", std::process::id()));
    let _ = fs::remove_dir_all(&output);
    for entry in walkdir::WalkDir::new(&source_cache) {
      let entry = entry.unwrap();
      let destination = output.join(entry.path().strip_prefix(&source_cache).unwrap());
      if entry.file_type().is_dir() {
        fs::create_dir_all(&destination).unwrap();
      } else {
        fs::copy(entry.path(), &destination).unwrap();
      }
    }

    let written =
      write_ymap_diffs(&output, &asset.join("vanilla"), &asset.join("vanilla-cache")).unwrap();
    let files = walkdir::WalkDir::new(output.join("diff"))
      .into_iter()
      .filter_map(std::result::Result::ok)
      .filter(|entry| entry.file_name().to_string_lossy().ends_with(".ymap.diff.json"))
      .count();
    println!("{written} diffs -> {}", output.join("diff").display());
    assert!(written > 0);
    assert_eq!(files, written);
    assert!(!output.join("diff.tmp").exists());
  }
}
