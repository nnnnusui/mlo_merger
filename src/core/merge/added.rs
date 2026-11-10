//! How the merge takes each YMAP that resources add, as the overwrite cache records it.
//!
//! A YMAP no vanilla YMAP is named after is merged from every resource's version that no
//! overwrite ignores, each compared with an empty YMAP, the one loaded last first; the
//! additions an earlier version makes already are left out (see
//! [`crate::core::format::ymap::compare::added`]). See [`crate::core::overwrite_cache`].

use crate::core::overwrite_cache::AddedYmap;
use std::collections::BTreeMap;

/// How one YMAP that resources add is merged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AddedPlan {
  /// The IDs of the resources it is merged from, in merge order.
  pub resources: Vec<String>,
  /// Whether several resources ship it, so the merged YMAP is always written to replace theirs.
  pub replacing: bool,
}

/// The plans of `files`, keyed by lowercase file name.
pub(super) fn plans(files: &BTreeMap<String, AddedYmap>) -> BTreeMap<String, AddedPlan> {
  files
    .iter()
    .map(|(name, file)| {
      (
        name.clone(),
        AddedPlan {
          resources: file.sources.iter().map(|source| source.resource.clone()).collect(),
          replacing: file.shipped_by.len() > 1,
        },
      )
    })
    .collect()
}
