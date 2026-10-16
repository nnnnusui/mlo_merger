//! Coordinates source-cache discovery, updates and publication.

use std::{
  collections::{BTreeMap, BTreeSet},
  path::PathBuf,
};

use super::{
  BuildSourceCache, Result, Staging,
  types::{FileFingerprint, ResourceCheck, ResourceInventory, SourceCacheMetadata},
};
use crate::core::vanilla_cache::LoadedCache;

pub(super) mod diff;
mod prepare;
mod publish;
mod resources;

struct SourceResources {
  source_dir: PathBuf,
  resources: Vec<PathBuf>,
  selected: Vec<PathBuf>,
  names: BTreeSet<String>,
  discovered: BTreeSet<String>,
  selected_only: bool,
}

struct VanillaInputs {
  vanilla_dir: PathBuf,
  vanilla_cache_dir: PathBuf,
  manifest_sha256: String,
  derived: LoadedCache,
  resource_revision: String,
  upstream_changed: bool,
}

struct CacheState {
  inventories: BTreeMap<String, ResourceInventory>,
  cached_inputs: BTreeMap<String, FileFingerprint>,
  checks: BTreeMap<String, ResourceCheck>,
  current_inputs: BTreeMap<String, FileFingerprint>,
  source_inputs: BTreeMap<String, FileFingerprint>,
}

struct Publication {
  output: PathBuf,
  previous: Option<SourceCacheMetadata>,
  generated_at: String,
  history: PathBuf,
  staging: Staging,
}

struct Prepared {
  source: SourceResources,
  vanilla: VanillaInputs,
  state: CacheState,
  publication: Publication,
  excluded_cache_paths: BTreeSet<PathBuf>,
  changed: bool,
}

pub(super) fn run(build: &BuildSourceCache) -> Result<bool> {
  let Prepared {
    source,
    vanilla,
    mut state,
    publication,
    excluded_cache_paths,
    mut changed,
  } = prepare::prepare(build)?;

  prepare::archive_excluded_resources(&publication, &excluded_cache_paths)?;
  changed |=
    resources::update_selected_resources(&source, &vanilla, &publication, &mut state, build.force)?;
  changed |= resources::remove_missing_resources(&source, &publication, &mut state)?;
  let output = publication.output.clone();
  let vanilla_dir = vanilla.vanilla_dir.clone();
  let vanilla_cache_dir = vanilla.vanilla_cache_dir.clone();
  let published = publish::publish_if_changed(source, vanilla, publication, state, changed)?;
  crate::core::config::matching::with_config(|| {
    diff::write_ymap_diffs(&output, &vanilla_dir, &vanilla_cache_dir)
  })??;
  Ok(published)
}
