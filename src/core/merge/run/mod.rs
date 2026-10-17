use super::cache_inputs::VanillaHistory;
use super::duplicates::{Change, Collector, Contribution, DuplicateReport};
use crate::common::function::collect_files_with_suffix;
use crate::core::config::blacklist::BlacklistConfig;
use crate::core::format::ymap::compare::{YmapDiffFields, YmapEntityDiff, merge_diffs};
use crate::core::format::ymap::diff::reference_hash;
use crate::core::format::ymap::model::ymap::Ymap;
use crate::core::format::ymap::relation::{EntityParentRef, entity_parent_ref};
use crate::core::format::ymap::xml::XmlYmap;
use crate::core::merge::{
  ymap_parent_cache::VanillaParentCache,
  ymap_parent_refs::{OriginalMap, ParentReferences, SourceMaps, patch_clone, runtime_entities},
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use struct_diff::Diff;

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

use inputs::{ModYmapReference, collect_modded_ymaps_map, latest_ymap_paths};
use plan::PlannedMap;

#[cfg(test)]
mod tests;

/// Logs how many items of each array a source changes.
fn log_diff_counts(diff: &YmapDiffFields) {
  let instanced = diff.instanced_data.as_ref();
  let counts = [
    ("entities", diff.entities.as_ref().map_or(0, Vec::len)),
    ("box_occluders", diff.box_occluders.as_ref().map_or(0, Vec::len)),
    ("occlude_models", diff.occlude_models.as_ref().map_or(0, Vec::len)),
    ("lod_lights", diff.lod_lights.as_ref().map_or(0, Vec::len)),
    (
      "distant_lod_lights",
      diff.distant_lod_lights.as_ref().and_then(|it| it.items.as_ref()).map_or(0, Vec::len),
    ),
    ("physics_dictionaries", diff.physics_dictionaries.as_ref().map_or(0, Vec::len)),
    ("prop_instances", instanced.and_then(|it| it.prop_instance_list.as_ref()).map_or(0, Vec::len)),
    ("grass_batches", instanced.and_then(|it| it.grass_instance_list.as_ref()).map_or(0, Vec::len)),
    ("time_cycle_modifiers", diff.time_cycle_modifiers.as_ref().map_or(0, Vec::len)),
    ("car_generators", diff.car_generators.as_ref().map_or(0, Vec::len)),
  ];
  let changed = counts
    .iter()
    .filter(|(_, count)| *count > 0)
    .map(|(name, count)| format!("{name}={count}"))
    .collect::<Vec<_>>();
  tracing::info!(
    "    Changed items: {}",
    if changed.is_empty() { "none".into() } else { changed.join(", ") }
  );
}
