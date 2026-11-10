use super::*;
use crate::core::vanilla::{
  CacheVersion, CachedFile, FileChange, VanillaCacheManifest, write_json,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Runs the vanilla, source and overwrite cache steps before `self`, as `--merge` does.
impl Merge {
  pub(crate) fn run_after_caches(
    &self,
    vanilla_dir: &Path,
  ) -> Result<MergeSummary> {
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
    if let Some(output_dir) = &self.overwrite_cache_dir {
      crate::core::overwrite_cache::BuildOverwriteCache {
        source_dir: self.source_dir.clone(),
        source_cache_dir: self.source_cache_dir.clone(),
        vanilla_cache_dir: self.vanilla_cache_dir.clone(),
        overwrite_dir: self.overwrite_dir.clone(),
        output_dir: output_dir.clone(),
      }
      .run()?;
    }
    self.run()
  }
}

mod duplicate_reports;
mod incremental_ybn;
mod incremental_ymap;
