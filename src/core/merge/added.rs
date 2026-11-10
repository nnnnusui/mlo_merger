//! How the merge takes each YMAP that resources add, as the overwrite cache records it.
//!
//! A YMAP no vanilla YMAP is named after is merged from one resource's, its base, against an
//! empty YMAP, as the source cache's diff document of it is taken. When an overwrite names the
//! base source, the base source's YMAP takes the place of vanilla instead: the other resources'
//! differences from it, which the overwrite cache holds, are merged into it, after the base
//! source's own overwrite as removals from it. See [`crate::core::overwrite_cache`].

use super::operation::Result;
use crate::core::overwrite::OverwriteFingerprint;
use crate::core::overwrite_cache::AddedYmap;
use crate::core::source_cache::{MergeInputs, MergeSourceFile};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// How one YMAP that resources add is merged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AddedPlan {
  /// The ID of the resource whose YMAP is the base.
  pub base: String,
  /// The IDs of the resources whose differences from the base source's YMAP are merged into it.
  pub additional: Vec<String>,
  /// Whether resources other than the base source ship the YMAP, so the merged YMAP is always
  /// written to replace theirs.
  pub replacing: bool,
  /// With [`Self::additional`], the base source's overwrite as removals from its YMAP, which
  /// takes the place of its own diff.
  pub base_diff: Option<PathBuf>,
}

/// Leaves in `inputs` only the sources `files` merges for each added YMAP, pointing the sources
/// merged into a base at their diffs in the overwrite cache at `cache_dir`; returns the plans
/// keyed by lowercase file name.
pub(super) fn apply(
  files: &BTreeMap<String, AddedYmap>,
  inputs: &mut MergeInputs,
  cache_dir: Option<&Path>,
) -> Result<BTreeMap<String, AddedPlan>> {
  let key = |resource: &str, stream_path: &str| {
    (resource.to_string(), stream_path.replace('\\', "/").to_ascii_lowercase())
  };
  let in_cache = |path: &str| -> Result<PathBuf> {
    Ok(cache_dir.ok_or("Base sources of added YMAPs need an overwrite cache")?.join(path))
  };
  let mut diffs = BTreeMap::new();
  let mut kept = BTreeMap::new();
  let mut plans = BTreeMap::new();
  for (name, file) in files {
    kept.insert(key(&file.base.resource, &file.base.stream_path), name.clone());
    for source in &file.sources {
      let diff = source.diff.as_ref().ok_or("Overwrite cache lists a source without a diff")?;
      kept.insert(key(&source.resource, &source.stream_path), name.clone());
      diffs.insert(key(&source.resource, &source.stream_path), in_cache(diff)?);
    }
    plans.insert(
      name.clone(),
      AddedPlan {
        base: file.base.resource.clone(),
        additional: file.sources.iter().map(|source| source.resource.clone()).collect(),
        replacing: file.shipped_by.len() > 1,
        base_diff: file.base_diff.as_deref().map(in_cache).transpose()?,
      },
    );
  }
  let added = inputs.added_ymaps.clone();
  inputs.ymap.retain(|source| {
    let name = source.file_name.to_ascii_lowercase();
    !added.contains(&name) || kept.get(&key(&source.resource, &source.stream_path)) == Some(&name)
  });
  for source in &mut inputs.ymap {
    if let Some(diff) = diffs.remove(&key(&source.resource, &source.stream_path)) {
      source.diff_path = Some(diff);
    }
  }
  Ok(plans)
}

/// The fingerprint the overwrite of `source` is written against as its diff's baseline, when
/// `source` is a source of an added YMAP in `plans`: the empty YMAP for a base source, the base
/// source's YMAP for one merged into it.
pub(super) fn overwrite_vanilla(
  plans: &BTreeMap<String, AddedPlan>,
  inputs: &MergeInputs,
  source: &MergeSourceFile,
) -> Result<Option<OverwriteFingerprint>> {
  let name = source.file_name.to_ascii_lowercase();
  let Some(plan) = plans.get(&name) else {
    return Ok(None);
  };
  if source.resource == plan.base {
    let empty = crate::core::source_cache::empty_baseline_file()?;
    return Ok(Some(OverwriteFingerprint {
      sha256: empty.sha256,
      size: empty.size,
    }));
  }
  let base = inputs
    .ymap
    .iter()
    .find(|other| other.resource == plan.base && other.file_name.to_ascii_lowercase() == name)
    .ok_or("Base source of an added YMAP is missing")?;
  Ok(Some(OverwriteFingerprint::read(&base.path)?))
}
