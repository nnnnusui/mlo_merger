//! Consistency of the derived vanilla cache with its metadata and the raw archive.

use super::{CACHE_SCHEMA, DerivedManifest, Result, generations::GenerationIndex};
use crate::core::check::{CheckIssueKind, Findings};
use crate::fs;
use std::{collections::BTreeSet, path::Path};

/// Entries of the cache directory besides `latest/`.
const CACHE_ENTRIES: [&str; 8] = [
  "cache_info.json",
  "ymap_relationships.json",
  "stream_file_rtree.json",
  "latest",
  "generations.json",
  // Units of every game build built so far, which the index of the selected one names some of.
  "generation-units",
  // Written once YMAP version diffs are generated again.
  "ymap_versions.json",
  "diff",
];

/// Checks that each latest file is a link to the archive object the metadata names, with the
/// recorded SHA-256 when `deep`, and that `latest/` holds nothing else.
pub(crate) fn check(
  dir: &Path,
  vanilla_dir: &Path,
  deep: bool,
  findings: &mut Findings,
) -> Result<()> {
  let manifest = fs::read(dir.join("cache_info.json"))
    .map_err(|error| error.to_string())
    .and_then(|bytes| {
      serde_json::from_slice::<DerivedManifest>(&bytes).map_err(|error| error.to_string())
    })
    .and_then(|manifest| match manifest.format_version {
      CACHE_SCHEMA => Ok(manifest),
      version => Err(format!("unsupported schema {version}")),
    });
  let manifest = match manifest {
    Ok(manifest) => manifest,
    Err(error) => {
      findings.add(CheckIssueKind::Unreadable, "cache_info.json", Some(error));
      return Ok(());
    }
  };
  let mut recorded = BTreeSet::new();
  for file in manifest.files.values() {
    let object = file.object.replace('\\', "/");
    recorded.insert(object.clone());
    let link = dir.join(&object);
    let Ok(metadata) = fs::symlink_metadata(&link) else {
      findings.add(CheckIssueKind::Missing, &object, None);
      continue;
    };
    if !metadata.file_type().is_symlink() {
      findings.add(CheckIssueKind::WrongLink, &object, Some("not a link".into()));
      continue;
    }
    let Ok(target) = fs::canonicalize(&link) else {
      let target = fs::read_link(&link)?;
      findings.add(CheckIssueKind::BrokenLink, &object, Some(target.display().to_string()));
      continue;
    };
    // The archive may be elsewhere, or absent, in which case only the link is checked.
    if let Ok(expected) = fs::canonicalize(vanilla_dir.join(&file.vanilla_object))
      && target != expected
    {
      findings.add(
        CheckIssueKind::WrongLink,
        &object,
        Some(format!("points to {} instead of {}", target.display(), file.vanilla_object)),
      );
      continue;
    }
    findings.recorded_file(dir, &object, None, &file.sha256, deep)?;
  }
  findings.unrecorded_below(dir, "latest", &recorded)?;
  match GenerationIndex::load(dir) {
    Ok(Some(index)) => {
      for unit in index.families.values().flat_map(|family| &family.units) {
        let path = format!("generation-units/{}.json", unit.key);
        if !fs::is_file(dir.join(&path)) {
          findings.add(CheckIssueKind::Missing, &path, None);
        }
      }
    }
    Ok(None) => {}
    Err(error) => {
      findings.add(CheckIssueKind::Unreadable, "generations.json", Some(error.to_string()));
    }
  }
  findings.unknown_entries(dir, &CACHE_ENTRIES)
}
