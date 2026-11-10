//! Earlier versions of the cached stream files, kept by content (`history/`).
//!
//! When a resource ships a new version of a stream file the cache holds, the version it replaces
//! is moved to `history/{resource}/{stream path}/{sha256}.{extension}`, with the diff document it
//! had then, if any, as `{sha256}.diff.json`. Overwrites name the version they were written
//! against by its SHA-256, so it is found again without recording anything more: see
//! [`version_path`] and [`diff_path`]. Nothing removes history.

use super::Result;
use crate::core::format::ymap::compare::YmapComparisonDiff;
use crate::platform::fs;
use std::path::Path;

/// The directory of the history below the source cache.
pub const HISTORY_DIR: &str = "history";

/// Where the version with `sha256` of the stream file at `stream_path` of the resource named
/// `resource` is kept, relative to the source cache.
pub fn version_path(
  resource: &str,
  stream_path: &str,
  sha256: &str,
) -> String {
  let stream_path = stream_path.replace('\\', "/");
  let extension = Path::new(&stream_path)
    .extension()
    .map(|extension| format!(".{}", extension.to_string_lossy()))
    .unwrap_or_default();
  format!("{HISTORY_DIR}/{resource}/{stream_path}/{sha256}{extension}")
}

/// Where the diff document of that version is kept, relative to the source cache.
pub fn diff_path(
  resource: &str,
  stream_path: &str,
  sha256: &str,
) -> String {
  format!("{HISTORY_DIR}/{resource}/{}/{sha256}.diff.json", stream_path.replace('\\', "/"))
}

/// Moves the cached file at `cached`, the version with `sha256` of `stream_path` of `resource`,
/// into the history of the cache at `output`, with a copy of its diff document when the cache
/// holds one describing it.
pub(super) fn archive(
  output: &Path,
  resource: &str,
  stream_path: &str,
  cached: &Path,
  sha256: &str,
) -> Result<()> {
  let version = output.join(version_path(resource, stream_path, sha256));
  fs::create_dir_all(version.parent().ok_or("History path has no parent")?)?;
  if fs::exists(&version).unwrap_or(false) {
    fs::remove_file(cached)?;
  } else {
    crate::common::filesystem::move_file(cached, &version)?;
  }
  let diff = output.join("diff").join(resource).join(format!("{stream_path}.diff.json"));
  let describes = crate::common::json::read_json::<YmapComparisonDiff>(&diff)
    .is_ok_and(|document| document.source.sha256 == sha256);
  if describes {
    fs::copy(&diff, output.join(diff_path(resource, stream_path, sha256)))?;
  }
  Ok(())
}
