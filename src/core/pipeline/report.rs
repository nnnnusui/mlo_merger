//! What a pipeline run reports.

use crate::common::changes::StoreChanges;
use crate::core::{deploy::DeploySummary, overwrite_cache::OverwriteWarnings};
use serde::Serialize;

/// Store files changed by each cached stage of a run.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct PipelineChanges {
  /// Latest vanilla files the derived vanilla cache names, as files of the vanilla archive; a
  /// file a newer stage replaces is removed and its replacement added.
  pub vanilla_cache: StoreChanges,
  /// Source stream files moved into the source cache.
  pub source_cache: StoreChanges,
  /// Merged stream files.
  pub merged: StoreChanges,
}

/// What the pipeline prints: [`PipelineChanges`] and the merge's [`OverwriteWarnings`].
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct PipelineReport {
  #[serde(flatten)]
  pub changes: PipelineChanges,
  /// Listed on every run until the overwrites or the files change.
  #[serde(flatten)]
  pub warnings: OverwriteWarnings,
}

/// Outcome of a pipeline run.
#[derive(Debug)]
pub struct PipelineSummary {
  /// Store files changed by the cached stages.
  pub changes: PipelineChanges,
  /// What the overwrites leave undecided or no longer match.
  pub warnings: OverwriteWarnings,
  /// File operations performed by the final deployment.
  pub deploy: DeploySummary,
}
