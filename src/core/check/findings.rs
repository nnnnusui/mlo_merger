//! Recording a store's issues, with the comparisons every store's check makes.

use super::{CheckIssue, CheckIssueKind, Result};
use crate::common::fingerprint::Fingerprint;
use crate::platform::fs;
use crate::platform::fs::WalkDir;
use std::{collections::BTreeSet, path::Path};

/// The issues found in one store.
pub(crate) struct Findings {
  store: &'static str,
  issues: Vec<CheckIssue>,
}

impl Findings {
  pub(crate) fn new(store: &'static str) -> Self {
    Self {
      store,
      issues: Vec::new(),
    }
  }

  pub(crate) fn into_issues(self) -> Vec<CheckIssue> {
    self.issues
  }

  /// Records an issue with `path`, relative to the store's directory.
  pub(crate) fn add(
    &mut self,
    kind: CheckIssueKind,
    path: impl AsRef<Path>,
    detail: Option<String>,
  ) {
    self.issues.push(CheckIssue {
      store: self.store.into(),
      kind,
      path: path.as_ref().to_string_lossy().replace('\\', "/"),
      detail,
    });
  }

  /// Records each file below `directory` (relative to `root`) that is not in `recorded`.
  pub(crate) fn unrecorded_below(
    &mut self,
    root: &Path,
    directory: &str,
    recorded: &BTreeSet<String>,
  ) -> Result<()> {
    for path in files_below(root, directory)? {
      if !recorded.contains(&path) {
        self.add(CheckIssueKind::Unrecorded, &path, None);
      }
    }
    Ok(())
  }

  /// Records each entry directly in `root` whose name is not in `known`.
  pub(crate) fn unknown_entries(
    &mut self,
    root: &Path,
    known: &[&str],
  ) -> Result<()> {
    for entry in fs::read_dir(root)? {
      let name = entry?.file_name().to_string_lossy().into_owned();
      if !known.contains(&name.as_str()) {
        self.add(CheckIssueKind::Unrecorded, &name, None);
      }
    }
    Ok(())
  }

  /// Records `relative` (below `root`) as missing or changed when it is not the file recorded
  /// with `size` (when known) and `sha256`, which is compared only when `deep`.
  pub(crate) fn recorded_file(
    &mut self,
    root: &Path,
    relative: &str,
    size: Option<u64>,
    sha256: &str,
    deep: bool,
  ) -> Result<()> {
    let path = root.join(relative);
    let Ok(metadata) = fs::metadata(&path) else {
      self.add(CheckIssueKind::Missing, relative, None);
      return Ok(());
    };
    if let Some(size) = size.filter(|size| *size != metadata.len()) {
      self.add(
        CheckIssueKind::Changed,
        relative,
        Some(format!("size {} instead of {size}", metadata.len())),
      );
    } else if deep {
      let actual = Fingerprint::read(&fs::canonicalize(&path)?, None, true)?.sha256;
      if actual != sha256 {
        self.add(CheckIssueKind::Changed, relative, Some(format!("SHA-256 {actual}")));
      }
    }
    Ok(())
  }
}

/// The files and links below `root/directory`, without following links, relative to `root`
/// with `/` separators; none when the directory does not exist.
pub(crate) fn files_below(
  root: &Path,
  directory: &str,
) -> Result<Vec<String>> {
  let start = root.join(directory);
  if !fs::is_dir(&start) {
    return Ok(Vec::new());
  }
  let mut files = Vec::new();
  for entry in WalkDir::new(&start).follow_links(false) {
    let entry = entry?;
    if entry.file_type().is_dir() {
      continue;
    }
    files.push(entry.path().strip_prefix(root)?.to_string_lossy().replace('\\', "/"));
  }
  Ok(files)
}
