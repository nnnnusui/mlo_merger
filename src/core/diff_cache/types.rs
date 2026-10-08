//! Internal candidate and generation-report data structures.

use crate::core::vanilla::CachedFile;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Clone)]
pub(super) struct Variant {
  pub(super) index: usize,
  pub(super) version: String,
  pub(super) file: CachedFile,
}

#[derive(Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(super) struct CandidateScore {
  pub(super) version: String,
  pub(super) sha256: String,
  pub(super) difference_count: usize,
}

#[derive(Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(super) struct FileReport {
  pub(super) input: String,
  pub(super) input_sha256: String,
  pub(super) best_version: String,
  pub(super) best_difference_count: usize,
  pub(super) candidates: Vec<CandidateScore>,
  pub(super) baseline_content_version: String,
  pub(super) baseline_sha256: String,
  pub(super) baseline_source: String,
  pub(super) difference_count: usize,
  pub(super) diff: String,
}

#[derive(Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(super) struct ResourceReport {
  pub(super) resource: String,
  pub(super) source: PathBuf,
  pub(super) generated_at: String,
  pub(super) vanilla_version: Option<String>,
  pub(super) scanned_files: usize,
  pub(super) unmatched_files: Vec<String>,
  #[serde(skip_serializing_if = "Vec::is_empty", default)]
  pub(super) unsupported_files: Vec<String>,
  pub(super) files: Vec<FileReport>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub(super) struct GenerationReport {
  pub(super) format_version: u32,
  pub(super) input: PathBuf,
  pub(super) vanilla: PathBuf,
  pub(super) generated_at: String,
  pub(super) completed: bool,
  pub(super) error: Option<String>,
  pub(super) distance_metric: &'static str,
  pub(super) resources: Vec<ResourceReport>,
}
