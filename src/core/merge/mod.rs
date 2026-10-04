pub mod run;
pub mod ybn_conflicts;
mod ymap_box_occluder_diff;
mod ymap_car_generator_diff;
mod ymap_diff;
mod ymap_distant_lod_light_diff;
mod ymap_entitiy_diff;
mod ymap_instanced_data_diff;
mod ymap_lod_light_diff;
mod ymap_metadata_diff;
mod ymap_occlude_model_diff;
mod ymap_parent_cache;
mod ymap_parent_refs;
mod ymap_time_cycle_modifier_diff;

/// Builds or updates the cached parent index for vanilla YMAP XML files.
pub fn build_ymap_parent_cache(vanilla_dir: &std::path::Path) -> std::io::Result<()> {
  ymap_parent_cache::VanillaParentCache::update(vanilla_dir).map(|_| ())
}
