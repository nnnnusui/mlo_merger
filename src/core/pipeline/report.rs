//! What a pipeline run reports.

use crate::common::changes::StoreChanges;
use crate::core::{deploy::DeploySummary, overwrite::IgnoredOverwrite};
use serde::Serialize;

/// Store files changed by each cached stage of a run.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct PipelineChanges {
  /// Latest vanilla files in the derived vanilla cache.
  pub vanilla_cache: StoreChanges,
  /// Source stream files moved into the source cache.
  pub source_cache: StoreChanges,
  /// Merged stream files.
  pub merged: StoreChanges,
}

/// What the pipeline prints: [`PipelineChanges`] and the overwrites the merge left out.
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct PipelineReport {
  #[serde(flatten)]
  pub changes: PipelineChanges,
  /// Overwrites not applied because the files they name are missing or changed; listed on
  /// every run until the overwrite or the files change.
  pub ignored_overwrites: Vec<IgnoredOverwrite>,
}

/// Outcome of a pipeline run.
#[derive(Debug)]
pub struct PipelineSummary {
  /// Store files changed by the cached stages.
  pub changes: PipelineChanges,
  /// Overwrites the merge left out.
  pub ignored_overwrites: Vec<IgnoredOverwrite>,
  /// File operations performed by the final deployment.
  pub deploy: DeploySummary,
}
