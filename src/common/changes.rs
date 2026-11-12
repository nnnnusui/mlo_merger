//! Files a stage added, changed or removed in the store it publishes.

use serde::Serialize;
use std::{
  collections::BTreeSet,
  path::{Path, PathBuf},
};

/// Files of one store whose contents a run added, changed or removed. A stage records paths
/// relative to its store directory; [`StoreChanges::under`] places them below it, as the
/// pipeline reports them.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct StoreChanges {
  /// Files the store did not hold before the run.
  pub added: BTreeSet<PathBuf>,
  /// Files whose contents changed.
  pub modified: BTreeSet<PathBuf>,
  /// Files the store no longer holds.
  pub removed: BTreeSet<PathBuf>,
}

impl StoreChanges {
  /// Records the file at `path` going from SHA-256 `before` to `after`, `None` meaning absent;
  /// combines with what the run already recorded for it, so a file removed and then added again
  /// is modified and one added and then removed is unchanged.
  pub(crate) fn record(
    &mut self,
    path: impl Into<PathBuf>,
    before: Option<&str>,
    after: Option<&str>,
  ) {
    let path = path.into();
    match (before, after) {
      (None, Some(_)) => {
        if self.removed.remove(&path) {
          self.modified.insert(path);
        } else {
          self.added.insert(path);
        }
      }
      (Some(_), None) => {
        self.modified.remove(&path);
        if !self.added.remove(&path) {
          self.removed.insert(path);
        }
      }
      (Some(before), Some(after)) if before != after && !self.added.contains(&path) => {
        self.modified.insert(path);
      }
      _ => {}
    }
  }

  /// Records the changes of `other`, recorded after those already here.
  pub(crate) fn merge(
    &mut self,
    other: Self,
  ) {
    for path in other.added {
      self.record(path, None, Some("after"));
    }
    for path in other.modified {
      self.record(path, Some("before"), Some("after"));
    }
    for path in other.removed {
      self.record(path, Some("before"), None);
    }
  }

  /// Whether the run changed no file.
  pub fn is_empty(&self) -> bool {
    self.added.is_empty() && self.modified.is_empty() && self.removed.is_empty()
  }

  /// The same changes with each path placed below `directory`.
  pub fn under(
    self,
    directory: &Path,
  ) -> Self {
    let join =
      |paths: BTreeSet<PathBuf>| paths.into_iter().map(|path| directory.join(path)).collect();
    Self {
      added: join(self.added),
      modified: join(self.modified),
      removed: join(self.removed),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn records_combine_into_one_change_per_file() {
    let mut changes = StoreChanges::default();
    changes.record("new", None, Some("a"));
    changes.record("changed", Some("a"), Some("b"));
    changes.record("same", Some("a"), Some("a"));
    changes.record("gone", Some("a"), None);
    changes.record("replaced", Some("a"), None);
    changes.record("replaced", None, Some("b"));
    changes.record("transient", None, Some("a"));
    changes.record("transient", Some("a"), None);
    assert_eq!(
      changes.under(Path::new("store")),
      StoreChanges {
        added: BTreeSet::from(["store/new".into()]),
        modified: BTreeSet::from(["store/changed".into(), "store/replaced".into()]),
        removed: BTreeSet::from(["store/gone".into()]),
      }
    );
  }
}
