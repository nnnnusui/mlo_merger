use super::*;
use crate::core::vanilla::{
  CacheVersion, CachedFile, FileChange, VanillaCacheManifest, write_json,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Runs the vanilla and source cache steps before `self`, as `--merge` does.
impl Merge {
  pub(crate) fn run_after_caches(
    &self,
    vanilla_dir: &Path,
  ) -> Result<()> {
    crate::core::source_cache::BuildSourceCache {
      source_dir: self.source_dir.clone(),
      output_dir: self.source_cache_dir.clone(),
      vanilla_dir: vanilla_dir.to_path_buf(),
      vanilla_cache_dir: self.vanilla_cache_dir.clone(),
      resource: None,
      exclusions: None,
      force: false,
    }
    .run_after_vanilla_cache()?;
    self.run()
  }
}

mod duplicate_reports;
mod incremental_ybn;
mod incremental_ymap;
mod path_validation;
