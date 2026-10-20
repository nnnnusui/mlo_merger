//! The merged stream files a merge output records, for readers outside the merge.

use std::path::{Path, PathBuf};

use super::incremental::{MergeMetadata, output_path};
use super::merge::Result;

/// The merge output's metadata, written with every published merge, so it changes whenever the
/// merged files do.
pub fn cache_info_path(merged_dir: &Path) -> PathBuf {
  merged_dir.join("merge_cache_info.json")
}

/// One merged stream file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedFile {
  /// Stream file name, such as `hei_ch1_11.ymap`.
  pub name: String,
  /// The merged file, below the merge output directory.
  pub path: PathBuf,
}

/// Lists the merged stream files recorded in the merge output's metadata, by name.
///
/// Files a merge left unchanged from vanilla have no output and are not listed.
pub fn merged_files(merged_dir: &Path) -> Result<Vec<MergedFile>> {
  let path = cache_info_path(merged_dir);
  if !path.is_file() {
    return Err(format!("Merge metadata is missing: {}", path.display()).into());
  }
  let metadata = MergeMetadata::load(merged_dir)?
    .ok_or_else(|| format!("Unsupported merge metadata schema: {}", path.display()))?;
  metadata
    .files
    .into_iter()
    .filter_map(|(name, record)| Some((name, record.output?)))
    .map(|(name, output)| {
      Ok(MergedFile {
        name,
        path: output_path(merged_dir, &output.path)?,
      })
    })
    .collect()
}
