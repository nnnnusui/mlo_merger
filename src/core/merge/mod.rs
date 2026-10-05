pub mod run;
pub mod ybn_conflicts;
mod ymap_parent_cache;
mod ymap_parent_refs;

pub use crate::core::format::ymap::diff::YmapDiff;

/// Builds or updates the cached parent index for vanilla YMAP XML files.
pub fn build_ymap_parent_cache(vanilla_dir: &std::path::Path) -> std::io::Result<()> {
  ymap_parent_cache::VanillaParentCache::update(vanilla_dir).map(|_| ())
}
