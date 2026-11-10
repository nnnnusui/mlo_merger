//! The overwrites that do not take effect, and the choices they leave to defaults
//! (`warnings.json`).

use super::{AddedYmap, AddedYmapChoice, Result, normalize, resource_name};
use crate::core::overwrite::{
  IgnoredOverwrite, IgnoredOverwriteReason, OverwriteFingerprint, Overwrites,
};
use crate::core::source_cache::MergeInputs;
use crate::platform::fs;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// What the overwrites leave undecided or no longer match, against the current source and
/// vanilla caches. It is current state, not a change: each entry is listed until the overwrites
/// or the files change.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct OverwriteWarnings {
  /// YMAPs that several resources add and whose base source no overwrite names, so the one
  /// loaded last wins.
  pub added_ymaps_without_base: Vec<AddedYmapWithoutBase>,
  /// Added YMAP overwrites whose base source does not ship the YMAP, or whose YMAP no resource
  /// ships.
  pub unmatched_base_sources: Vec<UnmatchedBaseSource>,
  /// Resource and vanilla overwrites not applied because the files they name are missing or
  /// differ from those they were written against, as after a source or vanilla update.
  pub ignored_overwrites: Vec<IgnoredOverwrite>,
}

/// A YMAP that several resources add, taken from the one loaded last.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct AddedYmapWithoutBase {
  /// The lowercase file name.
  pub file: String,
  /// The name of the resource whose YMAP wins.
  pub base: String,
  /// The names of every resource shipping the YMAP.
  pub shipped_by: Vec<String>,
}

/// An added YMAP overwrite whose base source is not among the resources shipping its YMAP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct UnmatchedBaseSource {
  /// The lowercase file name.
  pub file: String,
  /// The base source the overwrite names.
  pub base_source: String,
  /// The name of the resource whose YMAP wins instead; absent when no resource ships it, or
  /// every one that does is ignored.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub base: Option<String>,
}

impl OverwriteWarnings {
  /// Whether there is nothing to warn about.
  pub fn is_empty(&self) -> bool {
    self.added_ymaps_without_base.is_empty()
      && self.unmatched_base_sources.is_empty()
      && self.ignored_overwrites.is_empty()
  }

  /// Logs each entry as a warning.
  pub fn log(&self) {
    for added in &self.added_ymaps_without_base {
      tracing::warn!(
        "Added YMAP {} is shipped by {}; using {}, which loads last. Name the base source in added/ymap/{}.json of the overwrites to choose another",
        added.file,
        added.shipped_by.join(", "),
        added.base,
        added.file
      );
    }
    for unmatched in &self.unmatched_base_sources {
      match &unmatched.base {
        Some(base) => tracing::warn!(
          "Overwrite of added YMAP {} names base source {}, which does not ship it or is ignored; using {base}",
          unmatched.file,
          unmatched.base_source
        ),
        None => tracing::warn!(
          "Overwrite of added YMAP {} names base source {}, but no resource merges it",
          unmatched.file,
          unmatched.base_source
        ),
      }
    }
    for overwrite in &self.ignored_overwrites {
      let file = match &overwrite.resource {
        Some(resource) => format!("{} ({resource})", overwrite.file),
        None => format!("vanilla {}", overwrite.file),
      };
      let reason = match overwrite.reason {
        IgnoredOverwriteReason::MissingSource => "names no merged source YMAP",
        IgnoredOverwriteReason::SourceChanged => "was written against another source file",
        IgnoredOverwriteReason::MissingVanilla => "names no latest vanilla YMAP",
        IgnoredOverwriteReason::VanillaChanged => "was written against another vanilla file",
      };
      tracing::warn!("Overwrite of {file} {reason}; not applied");
    }
  }
}

/// The warnings for `overwrites` against `inputs`, as the source cache lists them, the choices
/// `files` record for added YMAPs, and the latest vanilla files in `vanilla_cache_dir`.
///
/// A resource overwrite is compared with its merged source YMAP and the baseline the merge
/// compares it with: the latest vanilla file of that name, or for an added YMAP the empty YMAP
/// (its base source) or the base source's YMAP (the others). A vanilla overwrite is compared with
/// the latest vanilla file.
pub fn collect(
  inputs: &MergeInputs,
  overwrites: &Overwrites,
  files: &BTreeMap<String, AddedYmap>,
  vanilla_cache_dir: &Path,
) -> Result<OverwriteWarnings> {
  let mut warnings = OverwriteWarnings::default();
  for (name, file) in files {
    if file.chosen_by == AddedYmapChoice::LastLoaded {
      warnings.added_ymaps_without_base.push(AddedYmapWithoutBase {
        file: name.clone(),
        base: resource_name(&file.base.resource),
        shipped_by: file.shipped_by.clone(),
      });
    }
  }
  for (name, overwrite) in overwrites.added_files() {
    let file = files.get(name);
    if file.is_none_or(|file| file.chosen_by != AddedYmapChoice::Overwrite) {
      warnings.unmatched_base_sources.push(UnmatchedBaseSource {
        file: name.clone(),
        base_source: overwrite.base_source.clone(),
        base: file.map(|file| resource_name(&file.base.resource)),
      });
    }
  }

  let latest = latest_ymaps(&vanilla_cache_dir.join("latest"))?;
  let key = |resource: &str, stream_path: &str| {
    (resource_name(resource).to_ascii_lowercase(), normalize(stream_path))
  };
  let shipped = inputs
    .ymap
    .iter()
    .map(|source| key(&source.resource, &source.stream_path))
    .collect::<BTreeSet<_>>();
  // Of the sources of an added YMAP, only its base and those merged into it are merged.
  let merged_added = files
    .values()
    .flat_map(|file| std::iter::once(&file.base).chain(&file.sources))
    .map(|source| key(&source.resource, &source.stream_path))
    .collect::<BTreeSet<_>>();
  let sources = inputs
    .ymap
    .iter()
    .filter(|source| {
      !inputs.added_ymaps.contains(&source.file_name.to_ascii_lowercase())
        || merged_added.contains(&key(&source.resource, &source.stream_path))
    })
    .map(|source| (key(&source.resource, &source.stream_path), source))
    .collect::<BTreeMap<_, _>>();
  for (resource, stream_path) in overwrites.keys() {
    let Some(overwrite) = overwrites.stream_file(resource, stream_path) else {
      continue;
    };
    let file_key = (resource.to_string(), stream_path.to_string());
    // One leaving out a stream file applies while a resource ships it.
    if overwrite.ignore && shipped.contains(&file_key) {
      continue;
    }
    let source = sources.get(&file_key);
    let vanilla = match source {
      Some(source) => match files.get(&source.file_name.to_ascii_lowercase()) {
        Some(file) if file.base.resource == source.resource => {
          let empty = crate::core::source_cache::empty_baseline_file()?;
          Some(OverwriteFingerprint {
            sha256: empty.sha256,
            size: empty.size,
          })
        }
        Some(file) => {
          let base = sources
            .get(&key(&file.base.resource, &file.base.stream_path))
            .ok_or("Base source of an added YMAP is missing")?;
          Some(OverwriteFingerprint::read(&base.path)?)
        }
        None => latest
          .get(&source.file_name.to_ascii_lowercase())
          .map(|path| OverwriteFingerprint::read(path))
          .transpose()?,
      },
      None => None,
    };
    let reason = match (source, vanilla) {
      (Some(source), Some(vanilla)) => {
        overwrite.ignored_because(&OverwriteFingerprint::read(&source.path)?, &vanilla)
      }
      _ => Some(IgnoredOverwriteReason::MissingSource),
    };
    if let Some(reason) = reason {
      warnings.ignored_overwrites.push(IgnoredOverwrite {
        resource: Some(resource.to_string()),
        file: stream_path.to_string(),
        reason,
      });
    }
  }
  for (name, overwrite) in overwrites.vanilla_files() {
    let reason = match latest.get(name) {
      Some(vanilla) => (overwrite.vanilla != OverwriteFingerprint::read(vanilla)?)
        .then_some(IgnoredOverwriteReason::VanillaChanged),
      None => Some(IgnoredOverwriteReason::MissingVanilla),
    };
    if let Some(reason) = reason {
      warnings.ignored_overwrites.push(IgnoredOverwrite {
        resource: None,
        file: name.clone(),
        reason,
      });
    }
  }
  Ok(warnings)
}

/// The latest vanilla YMAPs below `latest_dir`, keyed by lowercase file name.
fn latest_ymaps(latest_dir: &Path) -> Result<BTreeMap<String, PathBuf>> {
  let mut files = BTreeMap::new();
  if !fs::is_dir(latest_dir) {
    return Ok(files);
  }
  for entry in crate::platform::fs::WalkDir::new(latest_dir).follow_links(false) {
    let entry = entry?;
    let path = entry.path();
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
      continue;
    };
    let name = name.to_ascii_lowercase();
    if name.ends_with(".ymap") && fs::is_file(path) {
      files.insert(name, path.to_path_buf());
    }
  }
  Ok(files)
}
