use crate::platform::fs;
use std::{
  collections::{BTreeMap, BTreeSet},
  io::{BufReader, Read},
  path::{Component, Path, PathBuf},
};

use super::duplicates::{DuplicateReport, EntityDuplicate};
use super::operation::Result;
use super::{run::MergeYmap, ybn_conflicts::MergeYbnConflicts};
use crate::core::{
  format::ymap::reference_hash,
  overwrite::{Overwrites, StreamFileOverwrite, VanillaOverwrite},
  source_cache::{MergeInputs, MergeSourceFile},
  vanilla::write_json,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Bump when YMAP merge policy, encoding or cached reports change.
const YMAP_ALGORITHM_VERSION: u32 = 11;
/// Bump when YBN merge policy, encoding or cached reports change.
const YBN_ALGORITHM_VERSION: u32 = 9;

fn algorithm_versions() -> BTreeMap<String, u32> {
  BTreeMap::from([("ymap".into(), YMAP_ALGORITHM_VERSION), ("ybn".into(), YBN_ALGORITHM_VERSION)])
}

fn algorithm_version(extension: &str) -> Result<u32> {
  match extension {
    "ymap" => Ok(YMAP_ALGORITHM_VERSION),
    "ybn" => Ok(YBN_ALGORITHM_VERSION),
    _ => Err(format!("Unsupported merge file type: {extension}").into()),
  }
}

pub(super) struct IncrementalMerge<'a> {
  pub vanilla_cache: &'a Path,
  pub source_cache: &'a Path,
  pub output: &'a Path,
  pub staging: &'a Path,
  pub ybn: &'a [PathBuf],
  pub ymap: &'a [PathBuf],
  pub inputs: &'a MergeInputs,
  /// The overwrites applied to the source YMAPs' diffs.
  pub overwrites: &'a Overwrites,
  /// How each YMAP resources add is merged, keyed by lowercase file name.
  pub added: &'a BTreeMap<String, crate::core::overwrite_cache::AddedYmap>,
  pub force: bool,
}

impl IncrementalMerge<'_> {
  /// A latest vanilla file's path relative to the vanilla cache, as merge records name it.
  fn stored_vanilla_path(
    &self,
    path: &Path,
  ) -> PathBuf {
    path.strip_prefix(self.vanilla_cache).unwrap_or(path).to_path_buf()
  }

  /// The overwrite of a source YMAP, if any.
  fn overwrite(
    &self,
    source: &MergeSourceFile,
  ) -> Option<&StreamFileOverwrite> {
    if source.propagated || !source.file_name.to_ascii_lowercase().ends_with(".ymap") {
      return None;
    }
    self.overwrites.for_source(source)
  }

  /// The overwrite of the latest vanilla file `name` (lowercase), if it is a merged YMAP.
  fn vanilla_overwrite(
    &self,
    name: &str,
  ) -> Option<&VanillaOverwrite> {
    if !name.ends_with(".ymap") {
      return None;
    }
    self.overwrites.vanilla_file(name)
  }
}

mod cache;
mod executor;
mod filesystem;
mod graph;
mod run;

use cache::signature;
pub(super) use cache::{FileRecord, Fingerprint, InputFile, MergeMetadata, OutputFile, reusable};
pub(super) use filesystem::output_path;
use filesystem::{file_name, retain_file};
use graph::{Group, groups};

#[cfg(test)]
mod tests;
