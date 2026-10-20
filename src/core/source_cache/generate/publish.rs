//! Refreshes conflict and load-plan metadata and publishes changed cache reports.

use super::{CacheState, Publication, Result, SourceCacheMetadata, SourceResources, VanillaInputs};
use crate::common::json::write_json;
use crate::common::path::ensure_safe_path;
use crate::core::source_cache::{
  conflicts::conflicts,
  types::{SourceCacheConflictReport, VanillaMatch},
  ymap_plan,
};
use std::{collections::BTreeSet, fs, io::BufReader};

pub(super) fn publish_if_changed(
  source: SourceResources,
  vanilla: VanillaInputs,
  publication: Publication,
  state: CacheState,
  mut changed: bool,
) -> Result<bool> {
  let SourceResources {
    source_dir,
    resources,
    ..
  } = source;
  let VanillaInputs {
    vanilla_dir,
    vanilla_cache_dir,
    manifest_sha256,
    derived,
    ..
  } = vanilla;
  let Publication {
    output,
    generated_at,
    staging,
    ..
  } = publication;
  let CacheState {
    mut inventories,
    checks,
    source_inputs,
    ..
  } = state;
  let conflicts = conflicts(&source_dir, &output, &inventories, &resources)?;
  let previous_conflicts =
    fs::File::open(output.join("stream_conflicts.json")).ok().and_then(|file| {
      serde_json::from_reader::<_, SourceCacheConflictReport>(BufReader::new(file)).ok()
    });
  changed |= previous_conflicts.as_ref() != Some(&conflicts);
  if !changed {
    remove_legacy_exclusions(&output)?;
    return Ok(false);
  }
  for inventory in inventories.values_mut() {
    for file in inventory.files_by_format.values_mut().flatten() {
      file.vanilla = derived.files.get(&file.file_name).map(|vanilla| VanillaMatch {
        version: vanilla.version.clone(),
        sha256: vanilla.sha256.clone(),
        content_matches: vanilla.sha256 == file.sha256,
      });
    }
  }
  let plan = ymap_plan::build(&inventories);
  let vanilla_ymaps = ymap_plan::vanilla_ymaps_to_read(
    &plan.changed_source_parents,
    &derived.relationships,
    &derived.files,
  );
  let outputs = BTreeSet::from([
    "source_cache.log".into(),
    "stream_conflicts.json".into(),
    "vanilla_ymaps_to_read.json".into(),
  ]);
  let metadata = SourceCacheMetadata {
    format_version: 3,
    source_dir,
    vanilla_dir,
    vanilla_cache_dir,
    vanilla_manifest_sha256: manifest_sha256,
    vanilla_cache_revision: derived.revision,
    latest_vanilla_version: derived.latest_version,
    generated_at,
    source_inputs,
    resource_checks: checks,
    resources: inventories,
    scanned_stream_file_count: conflicts.report.scanned_file_count,
    conflict_count: conflicts.report.conflict_count,
    ymap_load_plan: plan,
    outputs,
  };
  write_json(&staging.0.join("stream_conflicts.json"), &conflicts)?;
  write_json(&staging.0.join("vanilla_ymaps_to_read.json"), &vanilla_ymaps)?;
  fs::write(
    staging.0.join("source_cache.log"),
    format!(
      "generated_at={}\nresources={}\nstream_files={}\nconflicts={}\n",
      metadata.generated_at,
      metadata.resources.len(),
      metadata.scanned_stream_file_count,
      metadata.conflict_count
    ),
  )?;
  write_json(&staging.0.join(crate::core::source_cache::CACHE_INFO), &metadata)?;
  for name in
    metadata.outputs.iter().map(String::as_str).chain([crate::core::source_cache::CACHE_INFO])
  {
    fs::rename(staging.0.join(name), output.join(name))?;
  }
  remove_legacy_exclusions(&output)?;
  Ok(true)
}

fn remove_legacy_exclusions(output: &std::path::Path) -> Result<()> {
  let path = output.join("source_exclusions.json");
  ensure_safe_path(output, &path)?;
  match fs::remove_file(path) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}
