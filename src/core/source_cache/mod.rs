//! Source stream inventory, vanilla matching, and conflict/load planning.

mod incremental;
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

use self::types::{SourceCacheConflictReport, SourceCacheMetadata};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// One source stream file selected for merge from the current source cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeSourceFile {
  /// Resource identifier recorded by the source inventory.
  pub resource: String,
  /// Absolute path to the source stream file.
  pub path: PathBuf,
  /// Original resource stream path before extraction into the cache.
  pub original_path: PathBuf,
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
  if !matches!(metadata.format_version, 2 | 3) {
    return Err(format!("Unsupported source cache schema {}", metadata.format_version).into());
  }
  let conflicts: SourceCacheConflictReport = serde_json::from_reader(BufReader::new(
    std::fs::File::open(cache_dir.join("stream_conflicts.json"))?,
  ))?;
  let conflict_paths = [".ymap", ".ybn"]
    .into_iter()
    .map(|extension| {
      let paths = conflicts
        .report
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
        let original_path = resource.source.join(&file.path);
        let source_path = file
          .cached_path
          .as_ref()
          .map_or_else(|| original_path.clone(), |path| cache_dir.join(path));
        let source_relative = if conflicts.source_conflicts.is_some() {
          file.cached_path.as_ref().map(|path| path.to_ascii_lowercase())
        } else {
          Some(
            original_path
              .strip_prefix(&metadata.source_dir)?
              .to_string_lossy()
              .replace('\\', "/")
              .to_ascii_lowercase(),
          )
        };
        let conflict = conflict_paths
          .get(extension.as_str())
          .is_some_and(|paths| source_relative.as_ref().is_some_and(|path| paths.contains(path)));
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
            original_path,
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
  /// Updates one named resource, or all discovered resources when no name is supplied.
  pub fn run_selected(
    &self,
    resource: Option<&str>,
  ) -> Result<bool> {
    incremental::run(self, resource)
  }

  /// Builds an inventory and conflict report, skipping when inputs are unchanged.
  pub fn run(&self) -> Result<bool> {
    self.run_selected(None)
  }
}

#[cfg(test)]
mod tests;
