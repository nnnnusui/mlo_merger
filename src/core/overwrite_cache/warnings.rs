//! The overwrites that do not take effect (`warnings.json`).

use super::{Result, normalize, resource_name};
use crate::core::overwrite::{
  IgnoredOverwrite, IgnoredOverwriteReason, OverwriteFingerprint, Overwrites,
};
use crate::core::source_cache::MergeInputs;
use crate::platform::fs;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// What does not take effect of the overwrites, against the current source and vanilla caches.
/// It is current state, not a change: each entry is listed until the overwrites or the files
/// change.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct OverwriteWarnings {
  /// Resource and vanilla overwrites not applied because the files they name are missing or
  /// differ from those they were written against, as after a source or vanilla update.
  pub ignored_overwrites: Vec<IgnoredOverwrite>,
}

impl OverwriteWarnings {
  /// Whether there is nothing to warn about.
  pub fn is_empty(&self) -> bool {
    self.ignored_overwrites.is_empty()
  }

  /// Logs each entry as a warning.
  pub fn log(&self) {
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

/// The warnings for `overwrites` against `inputs`, as the source cache lists them, and the
/// latest vanilla files in `vanilla_cache_dir`.
///
/// A resource overwrite is compared with its source YMAP and the baseline its diff is taken
/// against: the latest vanilla file of that name, or the empty YMAP for an added YMAP. A vanilla
/// overwrite is compared with the latest vanilla file.
pub fn collect(
  inputs: &MergeInputs,
  overwrites: &Overwrites,
  vanilla_cache_dir: &Path,
) -> Result<OverwriteWarnings> {
  let mut warnings = OverwriteWarnings::default();
  let latest = latest_ymaps(&vanilla_cache_dir.join("latest"))?;
  let key = |resource: &str, stream_path: &str| {
    (resource_name(resource).to_ascii_lowercase(), normalize(stream_path))
  };
  let shipped = inputs
    .ymap
    .iter()
    .map(|source| key(&source.resource, &source.stream_path))
    .collect::<BTreeSet<_>>();
  let sources = inputs
    .ymap
    .iter()
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
      Some(source) if inputs.added_ymaps.contains(&source.file_name.to_ascii_lowercase()) => {
        let empty = crate::core::source_cache::empty_baseline_file()?;
        Some(OverwriteFingerprint {
          sha256: empty.sha256,
          size: empty.size,
        })
      }
      Some(source) => latest
        .get(&source.file_name.to_ascii_lowercase())
        .map(|path| OverwriteFingerprint::read(path))
        .transpose()?,
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
