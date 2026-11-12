//! The YMAP merge, run in phases on a shared [`MergeRun`].

use super::light_pairs::ImpliedDiffs;
use super::propagated::PropagatedDiffs;
use super::*;
use crate::core::merge::duplicates::EntityDuplicate;

mod child_counts;
mod outputs;
mod parents;
mod plan;
mod sources;

/// The source YMAPs to merge, grouped by file name.
type SourcesByName = BTreeMap<String, Vec<ModYmapReference>>;

impl MergeYmap {
  pub(super) fn run_inner(
    &self,
    latest_files: &[PathBuf],
    raw_sources: &[SourceYmap],
  ) -> Result<(), Box<dyn std::error::Error>> {
    crate::core::config::matching::with_config(|| {
      self.run_configured_inner(latest_files, raw_sources)
    })?
  }

  /// Merges in phases: grouping the sources by YMAP, registering them and their parents, merging
  /// each YMAP's diffs, relinking the children of YMAPs whose entities moved, restoring parent
  /// indices, counting each parent's children, and deciding which YMAPs are written.
  fn run_configured_inner(
    &self,
    latest_files: &[PathBuf],
    raw_sources: &[SourceYmap],
  ) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!(
      "Merging {} source YMAP files against {} into {}",
      raw_sources.len(),
      self.vanilla_dir.display(),
      self.output_dir.display()
    );
    let mut run = MergeRun {
      merge: self,
      latest_files,
      vanilla_map_paths: latest_ymap_paths(latest_files),
      registered_maps: HashSet::new(),
      sources: SourceMaps::default(),
      references: ParentReferences::default(),
      planned: BTreeMap::new(),
      duplicate_files: BTreeMap::new(),
    };
    let mut by_name = run.group_sources(raw_sources)?;
    let vanilla_overwrites = run.vanilla_overwrites(&mut by_name)?;
    run.register_sources(&by_name)?;
    let (mut implied, mut propagated) = run.carried_diffs(&mut by_name)?;
    for (ymap_name, mod_refs) in &by_name {
      let hash = reference_hash(ymap_name.trim_end_matches(".ymap"));
      let vanilla_overwrite = vanilla_overwrites.get(&hash).copied();
      run.plan_ymap(ymap_name, mod_refs, vanilla_overwrite, &mut implied, &mut propagated)?;
    }
    let changed_layouts = run.relink_vanilla_children()?;
    let (child_counts, resolved_links) = run.restore_parent_indices()?;
    run.check_local_hierarchies(resolved_links)?;
    run.update_child_counts(&changed_layouts, &child_counts)?;
    run.decide_outputs()?;
    let MergeRun {
      planned,
      duplicate_files,
      ..
    } = run;
    self.write_outputs(planned)?;
    crate::core::vanilla::write_json(
      &self.output_dir.join(".duplicates.json"),
      &DuplicateReport {
        format_version: 1,
        files: duplicate_files,
      },
    )?;
    Ok(())
  }
}

/// What the phases of one merge share.
struct MergeRun<'a> {
  merge: &'a MergeYmap,
  latest_files: &'a [PathBuf],
  /// The latest vanilla YMAPs by name hash.
  vanilla_map_paths: HashMap<u32, PathBuf>,
  /// Each resource's YMAPs taken, by name hash.
  registered_maps: HashSet<(String, u32)>,
  sources: SourceMaps,
  references: ParentReferences,
  /// The YMAPs to write or keep, by name hash.
  planned: BTreeMap<u32, PlannedMap>,
  duplicate_files: BTreeMap<String, Vec<EntityDuplicate>>,
}
