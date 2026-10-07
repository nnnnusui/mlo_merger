//! Source stream inventory, vanilla matching, and conflict/load planning.

mod inventory;
mod publication;
mod types;
mod vanilla;
mod ymap_plan;

use std::{
  collections::{BTreeMap, BTreeSet},
  io::BufReader,
  path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::core::{
  stream_conflicts::{StreamConflictReport, scan_stream_conflicts},
  vanilla::{load_manifest, write_json},
  vanilla_cache::BuildVanillaCache,
};

use self::{
  inventory::{discover_resources, source_fingerprints, source_inventory},
  publication::{
    cache_is_current, cached_source_inputs, create_staging_directory, prepare_output_path,
    publish_directory, refresh_source_timestamps,
  },
  types::SourceCacheMetadata,
  vanilla::{revision as derived_cache_revision, validate_derived_cache},
  ymap_plan::build as build_ymap_load_plan,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// One source stream file selected for merge from the current source cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeSourceFile {
  /// Resource identifier recorded by the source inventory.
  pub resource: String,
  /// Absolute path to the source stream file.
  pub path: PathBuf,
  /// Lowercase stream filename.
  pub file_name: String,
}

/// Source inputs and vanilla YMAP closure selected by the source-cache analysis.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeInputs {
  /// Changed or conflicting source YMAPs with a latest vanilla baseline.
  pub ymap: Vec<MergeSourceFile>,
  /// Conflicting source YBNs with a latest vanilla baseline.
  pub ybn: Vec<MergeSourceFile>,
  /// Latest vanilla YMAP filenames required by the source load plan.
  pub vanilla_ymaps_to_read: BTreeSet<String>,
}

/// Loads merge inputs from a generated source cache without rescanning source streams.
pub fn load_merge_inputs(cache_dir: &Path) -> Result<MergeInputs> {
  let metadata: SourceCacheMetadata = serde_json::from_reader(BufReader::new(
    std::fs::File::open(cache_dir.join("source_cache_info.json"))?,
  ))?;
  if metadata.format_version != 2 {
    return Err(format!("Unsupported source cache schema {}", metadata.format_version).into());
  }
  let conflicts: StreamConflictReport = serde_json::from_reader(BufReader::new(
    std::fs::File::open(cache_dir.join("stream_conflicts.json"))?,
  ))?;
  let conflict_paths = [".ymap", ".ybn"]
    .into_iter()
    .map(|extension| {
      let paths = conflicts
        .conflicts
        .get(extension)
        .into_iter()
        .flatten()
        .flat_map(|conflict| conflict.paths.iter())
        .map(|path| path.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
      (extension, paths)
    })
    .collect::<BTreeMap<_, _>>();
  let required_ymap_sources = metadata
    .ymap_load_plan
    .changed_source_parents
    .iter()
    .chain(&metadata.ymap_load_plan.additional_source_children)
    .map(|source| (source.resource.clone(), source.source_path.to_ascii_lowercase()))
    .collect::<BTreeSet<_>>();
  let vanilla_ymaps_to_read = serde_json::from_reader(BufReader::new(std::fs::File::open(
    cache_dir.join("vanilla_ymaps_to_read.json"),
  )?))?;
  let mut inputs = MergeInputs {
    vanilla_ymaps_to_read,
    ..MergeInputs::default()
  };
  for (resource_id, resource) in metadata.resources {
    for (extension, files) in resource.files_by_format {
      let selected = match extension.as_str() {
        ".ymap" => &mut inputs.ymap,
        ".ybn" => &mut inputs.ybn,
        _ => continue,
      };
      for file in files {
        let Some(vanilla) = &file.vanilla else {
          continue;
        };
        let source_path = resource.source.join(&file.path);
        let source_relative = source_path
          .strip_prefix(&metadata.source_dir)?
          .to_string_lossy()
          .replace('\\', "/")
          .to_ascii_lowercase();
        let conflict = conflict_paths
          .get(extension.as_str())
          .is_some_and(|paths| paths.contains(&source_relative));
        let required_child = extension == ".ymap"
          && required_ymap_sources.contains(&(resource_id.clone(), file.path.to_ascii_lowercase()));
        let selected_for_merge = match extension.as_str() {
          ".ymap" => !vanilla.content_matches || conflict || required_child,
          ".ybn" => conflict,
          _ => false,
        };
        if selected_for_merge {
          selected.push(MergeSourceFile {
            resource: resource_id.clone(),
            path: source_path,
            file_name: file.file_name,
          });
        }
      }
    }
  }
  inputs.ymap.sort_by(|left, right| left.path.cmp(&right.path));
  inputs.ybn.sort_by(|left, right| left.path.cmp(&right.path));
  Ok(inputs)
}

struct Staging(PathBuf);

impl Drop for Staging {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.0);
  }
}

/// Builds or reuses a source processing inventory without generating diffs.
#[derive(Debug, Clone)]
pub struct BuildSourceCache {
  /// Root containing FiveM resources.
  pub source_dir: PathBuf,
  /// Output directory for resource inventories and conflict reports.
  pub output_dir: PathBuf,
  /// Raw versioned vanilla archive used for lineage metadata.
  pub vanilla_dir: PathBuf,
  /// Derived latest files and YMAP relationship index.
  pub vanilla_cache_dir: PathBuf,
  /// Rebuilds even when all inputs and upstream revisions are current.
  pub force: bool,
}

impl BuildSourceCache {
  /// Builds an inventory and conflict report, skipping when inputs are unchanged.
  pub fn run(&self) -> Result<bool> {
    let source_dir = self.source_dir.canonicalize()?;
    let vanilla_dir = self.vanilla_dir.canonicalize()?;
    log::info!("Ensuring derived vanilla cache is current");
    BuildVanillaCache {
      vanilla_dir: vanilla_dir.clone(),
      output_dir: self.vanilla_cache_dir.clone(),
      through_version: None,
      force: false,
    }
    .run()?;
    let vanilla_cache_dir = self.vanilla_cache_dir.canonicalize()?;

    log::info!("Discovering source resources in {}", source_dir.display());
    let resources = discover_resources(&source_dir)?;
    log::info!("Found {} source resources", resources.len());
    let output_dir =
      prepare_output_path(&self.output_dir, &[&source_dir, &vanilla_dir, &vanilla_cache_dir])?;
    let cached_source_inputs = cached_source_inputs(&output_dir, &source_dir);

    let raw_manifest_bytes = std::fs::read(vanilla_dir.join("cache_info.json"))?;
    let raw_manifest = load_manifest(&vanilla_dir)?;
    raw_manifest.versions.last().ok_or("Raw vanilla archive has no stages")?;
    let vanilla_manifest_sha256 = format!("{:x}", Sha256::digest(&raw_manifest_bytes));
    let (derived, relationships) =
      validate_derived_cache(&vanilla_cache_dir, &vanilla_manifest_sha256)?;
    let vanilla_cache_revision = derived_cache_revision(&vanilla_cache_dir)?;
    log::info!("Fingerprinting source inputs");
    let source_inputs = source_fingerprints(&source_dir, &resources, &cached_source_inputs)?;
    log::info!("Fingerprinting complete: {} files", source_inputs.len());

    if !self.force
      && cache_is_current(
        &output_dir,
        &source_dir,
        &vanilla_dir,
        &vanilla_cache_dir,
        &vanilla_manifest_sha256,
        &vanilla_cache_revision,
        &source_inputs,
      )?
    {
      refresh_source_timestamps(&output_dir, &source_inputs)?;
      log::info!("Source cache is current at {}", output_dir.display());
      return Ok(false);
    }

    log::info!("Building per-resource source inventory");
    let inventories = source_inventory(&source_dir, &resources, &source_inputs, &derived.files)?;
    log::info!("Scanning cross-resource stream filename conflicts");
    let conflicts = scan_stream_conflicts(&source_dir)?;
    log::info!("Found {} conflicting basenames", conflicts.conflict_count);
    log::info!("Calculating YMAP parent/child load closure");
    let ymap_load_plan = build_ymap_load_plan(&inventories);
    let vanilla_ymaps_to_read = ymap_plan::vanilla_ymaps_to_read(
      &ymap_load_plan.changed_source_parents,
      &relationships,
      &derived.files,
    );
    let scanned_stream_file_count = inventories
      .values()
      .flat_map(|resource| resource.files_by_format.values())
      .map(Vec::len)
      .sum();
    log::info!(
      "YMAP load plan: {} changed parents, {} source children, {} vanilla files",
      ymap_load_plan.changed_source_parents.len(),
      ymap_load_plan.additional_source_children.len(),
      vanilla_ymaps_to_read.len()
    );

    let staging = Staging(create_staging_directory(&output_dir)?);
    let outputs = BTreeSet::from([
      "source_cache.log".to_owned(),
      "stream_conflicts.json".to_owned(),
      "vanilla_ymaps_to_read.json".to_owned(),
    ]);
    let generated_at = chrono::Utc::now().to_rfc3339();
    write_json(&staging.0.join("vanilla_ymaps_to_read.json"), &vanilla_ymaps_to_read)?;
    let log_path = staging.0.join("source_cache.log");
    let mut generation_log = std::io::BufWriter::new(std::fs::File::create(&log_path)?);
    use std::io::Write;
    writeln!(generation_log, "generated_at={generated_at}")?;
    writeln!(generation_log, "latest_vanilla_version={}", derived.latest_version)?;
    writeln!(generation_log, "resources={}", resources.len())?;
    writeln!(generation_log, "stream_files={scanned_stream_file_count}")?;
    writeln!(generation_log, "conflicts={}", conflicts.conflict_count)?;
    writeln!(
      generation_log,
      "changed_source_parents={}",
      ymap_load_plan.changed_source_parents.len()
    )?;
    writeln!(
      generation_log,
      "additional_source_children={}",
      ymap_load_plan.additional_source_children.len()
    )?;
    writeln!(generation_log, "vanilla_ymaps_to_read={}", vanilla_ymaps_to_read.len())?;
    generation_log.flush()?;
    write_json(&staging.0.join("stream_conflicts.json"), &conflicts)?;
    write_json(
      &staging.0.join("source_cache_info.json"),
      &SourceCacheMetadata {
        format_version: 2,
        source_dir: source_dir.clone(),
        vanilla_dir,
        vanilla_cache_dir,
        vanilla_manifest_sha256,
        vanilla_cache_revision,
        latest_vanilla_version: derived.latest_version,
        generated_at,
        source_inputs,
        resources: inventories,
        scanned_stream_file_count,
        conflict_count: conflicts.conflict_count,
        ymap_load_plan,
        outputs,
      },
    )?;
    log::info!("Publishing source cache atomically to {}", output_dir.display());
    publish_directory(&staging.0, &output_dir)?;
    log::info!("Built source inventory for {} resources", resources.len());
    Ok(true)
  }
}

#[cfg(test)]
mod tests;
