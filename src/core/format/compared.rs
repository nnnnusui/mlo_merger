//! The files a diff document compares, recorded as they were when it was taken, whatever their
//! stream file type.

use crate::platform::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::common::{Result, fingerprint::Fingerprint};

/// A stream file of a resource that changes are carried over from, and its vanilla baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct PropagationOrigin {
  pub target: ComparedFile,
  pub base: ComparedFile,
}

/// A compared stream file and its fingerprint when the diff was taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct ComparedFile {
  /// The file relative to the store holding it: the source cache, the vanilla cache, or the
  /// source root for a source file that was not moved into the cache.
  pub path: String,
  pub sha256: String,
  pub size: u64,
  pub modified_seconds: u64,
  pub modified_nanos: u32,
}

impl ComparedFile {
  /// Whether the file at `path` (after resolving links) has the recorded contents.
  ///
  /// The document names the file, so where it is does not matter: a file whose size and
  /// modification time match is not hashed again.
  pub fn describes(
    &self,
    path: &Path,
  ) -> Result<bool> {
    let path = fs::canonicalize(path)?;
    let previous = Fingerprint {
      sha256: self.sha256.clone(),
      size: self.size,
      modified_seconds: self.modified_seconds,
      modified_nanos: self.modified_nanos,
    };
    let current = Fingerprint::read(&path, Some(&previous), false)?;
    Ok(current.sha256 == self.sha256 && current.size == self.size)
  }

  /// Whether `other` has the same contents, wherever it is and whenever it was modified.
  pub fn same_content(
    &self,
    other: &Self,
  ) -> bool {
    self.sha256 == other.sha256 && self.size == other.size
  }
}
