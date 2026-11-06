//! The operating system's filesystem.

use std::{
  io,
  path::{Path, PathBuf},
};

pub use std::fs::*;

/// Recursive directory traversal.
pub mod walk {
  pub use walkdir::{DirEntry, Error, WalkDir};
}
pub use walk::WalkDir;

/// Whether `path` is an existing file, following symlinks, like [`Path::is_file`].
pub fn is_file(path: impl AsRef<Path>) -> bool {
  path.as_ref().is_file()
}

/// Whether `path` is an existing directory, following symlinks, like [`Path::is_dir`].
pub fn is_dir(path: impl AsRef<Path>) -> bool {
  path.as_ref().is_dir()
}

/// Creates a symlink at `link` pointing to the file `target`.
#[cfg(unix)]
pub fn symlink_file(
  target: impl AsRef<Path>,
  link: impl AsRef<Path>,
) -> io::Result<()> {
  std::os::unix::fs::symlink(target, link)
}

/// Creates a symlink at `link` pointing to the file `target`.
#[cfg(windows)]
pub fn symlink_file(
  target: impl AsRef<Path>,
  link: impl AsRef<Path>,
) -> io::Result<()> {
  std::os::windows::fs::symlink_file(target, link)
}

/// The process's working directory, against which relative paths resolve.
pub fn current_dir() -> io::Result<PathBuf> {
  std::env::current_dir()
}

/// An identifier unique to this process, used to name staging files.
pub fn process_id() -> u32 {
  std::process::id()
}
