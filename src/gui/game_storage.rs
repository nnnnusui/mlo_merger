//! Finding and checking FiveM's `game-storage` directory.

use std::path::{Path, PathBuf};

/// The name prefixes FiveM stores a game build's `update/update.rpf` under, followed by its SHA-1.
const UPDATE_RPF_PREFIXES: [&str; 2] = ["update+update.rpf_", "override+update+update.rpf_"];

/// What a chosen directory looks like to vanilla generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameStorageStatus {
  /// Holds this many game builds' `update.rpf`.
  UpdateRpfs(usize),
  /// A directory without any `update.rpf`.
  Empty,
  /// Not a readable directory.
  Missing,
}

/// Classifies `dir` by the `update.rpf` copies it holds.
pub fn status(dir: &Path) -> GameStorageStatus {
  let Ok(entries) = std::fs::read_dir(dir) else {
    return GameStorageStatus::Missing;
  };
  let count = entries
    .flatten()
    .filter(|entry| {
      let name = entry.file_name();
      let name = name.to_string_lossy();
      UPDATE_RPF_PREFIXES.iter().any(|prefix| name.starts_with(prefix))
    })
    .count();
  if count == 0 { GameStorageStatus::Empty } else { GameStorageStatus::UpdateRpfs(count) }
}

/// FiveM's game-storage directory in its default install location, when it holds an `update.rpf`.
pub fn detect() -> Option<PathBuf> {
  let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
  let dir = local.join("FiveM").join("FiveM.app").join("data").join("game-storage");
  matches!(status(&dir), GameStorageStatus::UpdateRpfs(_)).then_some(dir)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn status_counts_update_rpfs() {
    let root = std::env::temp_dir().join(format!("mlo_merger_storage_{}", std::process::id()));
    assert_eq!(status(&root), GameStorageStatus::Missing);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("other_file"), b"").unwrap();
    assert_eq!(status(&root), GameStorageStatus::Empty);
    std::fs::write(root.join("update+update.rpf_0123abcd"), b"").unwrap();
    std::fs::write(root.join("override+update+update.rpf_4567ef01"), b"").unwrap();
    assert_eq!(status(&root), GameStorageStatus::UpdateRpfs(2));
    std::fs::remove_dir_all(root).unwrap();
  }
}
