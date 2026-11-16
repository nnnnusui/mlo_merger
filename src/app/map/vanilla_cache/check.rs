//! Consistency of the derived vanilla cache with its metadata and the raw archive.

use super::{CACHE_SCHEMA, DerivedManifest, Result, generations::GenerationIndex};
use crate::core::check::{CheckIssueKind, Findings};
use crate::platform::fs;
use std::path::Path;

/// Entries of the cache directory.
const CACHE_ENTRIES: [&str; 7] = [
  "cache_info.json",
  "ymap_relationships.json",
  "stream_file_rtree.json",
  "generations.json",
  // Units of every game build built so far, which the index of the selected one names some of.
  "generation-units",
  // Written once YMAP version diffs are generated again.
  "ymap_versions.json",
  "diff",
];

/// Checks that each latest file is in the vanilla archive the metadata names, with the recorded
/// SHA-256 when `deep`. Files are reported by their path from the cache directory.
pub(crate) fn check(
  dir: &Path,
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
  for file in manifest.files.values() {
    let path = Path::new(&manifest.vanilla_dir).join(&file.object);
    findings.recorded_file(dir, &path.to_string_lossy(), None, &file.sha256, deep)?;
  }
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
