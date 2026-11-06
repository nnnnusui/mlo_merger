use super::BuildSourceCache;
use super::{types::*, ymap_plan};
use crate::core::vanilla::{CacheVersion, VanillaCacheManifest, write_json};
use crate::core::vanilla_cache::{DerivedVanillaFile, YmapRelationshipIndex};
use crate::fs;
use std::collections::{BTreeMap, BTreeSet};

/// Runs the vanilla cache step before `self`, as `--generate-source-cache` does.
impl BuildSourceCache {
  pub(crate) fn run_after_vanilla_cache(&self) -> super::Result<bool> {
    crate::core::vanilla_cache::BuildVanillaCache {
      vanilla_dir: self.vanilla_dir.clone(),
      output_dir: self.vanilla_cache_dir.clone(),
      gamebuild: None,
      force: false,
    }
    .run()?;
    self.run().map(|result| result.published)
  }
}

mod diff;
mod inventory;
mod migration;
mod restore;
mod selection;
mod timestamps;
