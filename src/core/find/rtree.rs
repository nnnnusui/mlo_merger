use super::{EntityQuery, FindEntity, Result, file_from_position, file_from_position::Candidate};
#[cfg(not(test))]
use indicatif::ProgressDrawTarget;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Serialize;
#[cfg(not(test))]
use std::io::IsTerminal;
use std::{
  path::{Path, PathBuf},
  time::Duration,
};

#[derive(Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
struct RTreeIndex {
  #[serde(rename = "schemaVersion")]
  schema_version: u32,
  #[serde(rename = "fileType")]
  file_type: &'static str,
  timestamp: String,
  #[serde(rename = "fileCount")]
  file_count: usize,
  #[serde(rename = "mergedFileCount")]
  merged_file_count: usize,
  #[serde(rename = "sourceFileCount")]
  source_file_count: usize,
  #[serde(rename = "vanillaFileCount")]
  vanilla_file_count: usize,
  merged: Vec<RTreeItem>,
  source: Vec<RTreeItem>,
  vanilla: Vec<RTreeItem>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
struct RTreeItem {
  #[serde(rename = "minX")]
  min_x: f64,
  #[serde(rename = "minY")]
  min_y: f64,
  #[serde(rename = "minZ")]
  min_z: f64,
  #[serde(rename = "maxX")]
  max_x: f64,
  #[serde(rename = "maxY")]
  max_y: f64,
  #[serde(rename = "maxZ")]
  max_z: f64,
  name: String,
  path: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  resource: Option<String>,
  #[serde(rename = "originalPath", skip_serializing_if = "Option::is_none")]
  original_path: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  version: Option<String>,
}

/// Generates RBush-compatible 2D boxes with retained Z bounds for YMAP and YBN streams.
///
/// Writes `stream_file_rtree-ymap.json` and `stream_file_rtree-ybn.json` under `output_dir`.
pub(crate) fn generate(output_dir: &Path) -> Result<Vec<PathBuf>> {
  #[cfg(not(test))]
  let log_progress = !std::io::stderr().is_terminal();
  #[cfg(test)]
  let log_progress = false;
  #[cfg(test)]
  let progress = ProgressBar::hidden();
  #[cfg(not(test))]
  let progress = ProgressBar::with_draw_target(Some(0), ProgressDrawTarget::stderr());
  progress.set_style(ProgressStyle::with_template("{spinner:.green} {msg}")?);
  progress.enable_steady_tick(Duration::from_millis(100));
  progress.set_message("Collecting stream-file bounds");
  let search = FindEntity {
    query: EntityQuery::FileFromPosition {
      position: [0.0; 3],
    },
    merged_dir: PathBuf::from("asset/merged"),
    filter: None,
    diff_all: true,
  };
  let candidates = file_from_position::collect_candidates(&search, &progress)?;
  let total = candidates.len();
  progress.set_length(total as u64);
  progress.set_style(
    ProgressStyle::with_template(
      "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
    )?
    .progress_chars("##-"),
  );
  if log_progress {
    eprintln!("generate-rtree-json: indexing {total} stream files");
  }
  let timestamp = chrono::Utc::now().to_rfc3339();
  let mut ymap = Groups::default();
  let mut ybn = Groups::default();
  let mut skipped = 0usize;
  for candidate in candidates {
    progress.set_message(candidate.path.display().to_string());
    let groups = match candidate.extension {
      "ymap" => &mut ymap,
      "ybn" => &mut ybn,
      _ => continue,
    };
    match index_item(&candidate) {
      Ok(Some(item)) => groups.push(candidate.stage, item),
      Ok(None) => skipped += 1,
      Err(error) if candidate.strict => return Err(error),
      Err(error) => {
        skipped += 1;
        progress.println(format!("Warning: skipped {}: {error}", candidate.path.display()));
      }
    }
    progress.inc(1);
    if log_progress && progress.position().is_multiple_of(1000) {
      eprintln!("generate-rtree-json: indexed {}/{} files", progress.position(), total);
    }
  }
  let checked = progress.position();
  progress.finish_and_clear();
  std::fs::create_dir_all(output_dir)?;
  let outputs = vec![
    write_index(output_dir, "ymap", timestamp.clone(), ymap)?,
    write_index(output_dir, "ybn", timestamp, ybn)?,
  ];
  eprintln!(
    "generate-rtree-json: checked {checked} candidates, skipped {skipped}, wrote {} indexes",
    outputs.len()
  );
  Ok(outputs)
}

#[derive(Default)]
struct Groups {
  merged: Vec<RTreeItem>,
  source: Vec<RTreeItem>,
  vanilla: Vec<RTreeItem>,
}

impl Groups {
  fn push(
    &mut self,
    stage: &str,
    item: RTreeItem,
  ) {
    match stage {
      "merged" => self.merged.push(item),
      "source" | "source-cache" => self.source.push(item),
      "vanilla" | "vanilla-cache" => self.vanilla.push(item),
      _ => {}
    }
  }

  fn into_index(
    self,
    file_type: &'static str,
    timestamp: String,
  ) -> RTreeIndex {
    let file_count = self.merged.len() + self.source.len() + self.vanilla.len();
    RTreeIndex {
      schema_version: 1,
      file_type,
      timestamp,
      file_count,
      merged_file_count: self.merged.len(),
      source_file_count: self.source.len(),
      vanilla_file_count: self.vanilla.len(),
      merged: self.merged,
      source: self.source,
      vanilla: self.vanilla,
    }
  }
}

fn index_item(candidate: &Candidate) -> Result<Option<RTreeItem>> {
  let Some(bounds) = file_from_position::spatial_bounds(candidate)? else {
    return Ok(None);
  };
  Ok(Some(RTreeItem {
    min_x: bounds.minimum[0],
    min_y: bounds.minimum[1],
    min_z: bounds.minimum[2],
    max_x: bounds.maximum[0],
    max_y: bounds.maximum[1],
    max_z: bounds.maximum[2],
    name: candidate.name.clone(),
    path: candidate.path.to_string_lossy().replace('\\', "/"),
    resource: candidate.resource.clone(),
    original_path: candidate.original_path.clone(),
    version: candidate.version.clone(),
  }))
}

fn write_index(
  output_dir: &Path,
  file_type: &'static str,
  timestamp: String,
  groups: Groups,
) -> Result<PathBuf> {
  let output = output_dir.join(format!("stream_file_rtree-{file_type}.json"));
  crate::core::vanilla::write_json(&output, &groups.into_index(file_type, timestamp))?;
  Ok(output)
}
