//! The merged stream files a merge output records, for readers outside the merge.

use crate::platform::fs;
use std::{
  collections::BTreeSet,
  path::{Path, PathBuf},
};

use super::incremental::{MergeMetadata, output_path};
use super::operation::Result;

/// File name of the merge output's metadata.
pub(crate) const CACHE_INFO: &str = "cache_info.json";

/// The merge output's metadata, written with every published merge, so it changes whenever the
/// merged files do.
pub fn cache_info_path(merged_dir: &Path) -> PathBuf {
  merged_dir.join(CACHE_INFO)
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
  if !fs::is_file(&path) {
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

/// The source paths, relative to the source root, of the stream files the merge omits without
/// writing a file of their name: those of a file left out for a newer generation
/// (`superseded_by` in the merge output's metadata), and those made from an older vanilla version
/// of a file the merge leaves as vanilla has it. They are not to be deployed. Without merge
/// metadata, there are none.
pub fn omitted_sources(merged_dir: &Path) -> Result<BTreeSet<String>> {
  let path = cache_info_path(merged_dir);
  if !fs::is_file(&path) {
    return Ok(BTreeSet::new());
  }
  let metadata = MergeMetadata::load(merged_dir)?
    .ok_or_else(|| format!("Unsupported merge metadata schema: {}", path.display()))?;
  Ok(
    metadata
      .files
      .into_values()
      .filter(|record| record.output.is_none())
      .flat_map(|record| {
        let superseded = record.superseded_by.is_some();
        record.merge_sources.into_iter().filter(move |input| superseded || input.made_from_older)
      })
      .filter_map(|input| input.original_path)
      .map(|original| original.to_string_lossy().replace('\\', "/"))
      .collect(),
  )
}
