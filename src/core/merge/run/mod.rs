use super::cache_inputs::VanillaHistory;
use super::duplicates::{Change, Collector, Contribution, DuplicateReport};
use crate::core::common::function::collect_files_with_suffix;
use crate::core::config::blacklist::BlacklistConfig;
use crate::core::diff_cache::ymap_distance;
use crate::core::extract::ExtractYmap;
use crate::core::format::ymap::diff::{YmapDiff, reference_hash};
use crate::core::format::ymap::model::ymap::Ymap;
use crate::core::format::ymap::xml::XmlYmap;
use crate::core::merge::{
  ymap_parent_cache::VanillaParentCache,
  ymap_parent_refs::{
    OriginalMap, ParentReferences, SourceMaps, patch_clone, runtime_entities, valid_local_pair,
  },
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct MergeYmap {
  pub vanilla_dir: PathBuf,
  pub mod_dir: PathBuf,
  pub mod_ymap_dir: PathBuf,
  pub output_dir: PathBuf,
  pub rebuild_all: bool,
  pub blacklist_config: Option<PathBuf>,
}

impl MergeYmap {
  pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
    self.run_inner(None, None, None)
  }

  /// Version-aware YMAP merge API for the planned historical-baseline workflow.
  pub fn run_with_vanilla_cache(
    &self,
    history: &VanillaHistory,
  ) -> Result<(), Box<dyn std::error::Error>> {
    self.run_inner(Some(history), None, None)
  }

  /// Merges source maps against the supplied latest vanilla files and emits edited RSC7 binaries.
  pub fn run_with_latest_vanilla_files(
    &self,
    vanilla_files: &[PathBuf],
    source_files: Option<&[(String, PathBuf)]>,
  ) -> Result<(), Box<dyn std::error::Error>> {
    self.run_inner(None, Some(vanilla_files), source_files)
  }
}

mod engine;
mod inputs;
mod output;
mod plan;

use inputs::{
  ModYmapReference, best_cached_ymap_diff, collect_modded_ymaps_map, latest_ymap_paths,
};
use plan::PlannedMap;

#[cfg(test)]
mod tests;
