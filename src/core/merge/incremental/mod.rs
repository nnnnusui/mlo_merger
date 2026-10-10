use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  io::{BufReader, Read},
  path::{Component, Path, PathBuf},
};

use super::duplicates::{DuplicateReport, EntityDuplicate};
use super::merge::Result;
use super::{run::MergeYmap, ybn_conflicts::MergeYbnConflicts};
use crate::core::{
  format::ymap::diff::reference_hash,
  source_cache::{MergeInputs, MergeSourceFile},
  vanilla::write_json,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Bump when YMAP merge policy, encoding or cached reports change.
const YMAP_ALGORITHM_VERSION: u32 = 6;
/// Bump when YBN merge policy, encoding or cached reports change.
const YBN_ALGORITHM_VERSION: u32 = 7;

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
  pub source_dir: &'a Path,
  pub vanilla_cache: &'a Path,
  pub source_cache: &'a Path,
  pub output: &'a Path,
  pub staging: &'a Path,
  pub ybn: &'a [PathBuf],
  pub ymap: &'a [PathBuf],
  pub inputs: &'a MergeInputs,
  pub force: bool,
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
