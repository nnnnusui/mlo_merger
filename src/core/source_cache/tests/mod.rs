use super::BuildSourceCache;
use super::{types::*, ymap_plan};
use crate::core::vanilla::{CacheVersion, VanillaCacheManifest, write_json};
use crate::core::vanilla_cache::{DerivedVanillaFile, YmapRelationshipIndex};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

/// Runs the vanilla cache step before `self`, as `--generate-source-cache` does.
impl BuildSourceCache {
  pub(crate) fn run_after_vanilla_cache(&self) -> super::Result<bool> {
    crate::core::vanilla_cache::BuildVanillaCache {
      vanilla_dir: self.vanilla_dir.clone(),
      output_dir: self.vanilla_cache_dir.clone(),
      through_version: None,
      force: false,
    }
    .run()?;
    self.run()
  }
}

mod diff;
mod inventory;
mod migration;
mod selection;
mod timestamps;
