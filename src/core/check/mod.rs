//! Read-only consistency checks of every store against its own metadata.
//!
//! Each store's module checks its own files, as it owns their layout: recorded files that are
//! missing or changed, files its metadata does not record, and, for the vanilla cache, links
//! that are broken or point elsewhere. Nothing is repaired; the next run of the store's stage
//! does that, except for unrecorded files, which no stage removes.

mod findings;

use serde::{Deserialize, Serialize};

pub(crate) use findings::{Findings, files_below};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// What [`Check::run`] found.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct CheckReport {
  pub issues: Vec<CheckIssue>,
}

/// One inconsistency between a store and its metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct CheckIssue {
  /// `vanilla`, `vanilla-cache`, `source-cache`, `overwrite-cache`, `merged` or `deployment`.
  pub store: String,
  pub kind: CheckIssueKind,
  /// The file, relative to the store's directory.
  pub path: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum CheckIssueKind {
  /// The metadata records the file, but it does not exist.
  Missing,
  /// The file exists, but the metadata does not record it.
  Unrecorded,
  /// The file's size, or with `--deep` its SHA-256, differs from the record.
  Changed,
  /// The metadata cannot be read, so the store was not checked further.
  Unreadable,
  /// The store was built from inputs that changed since; its stage rebuilds it.
  Stale,
}
