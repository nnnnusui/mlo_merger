//! The process the library runs in: its working directory and identifier.
//!
//! The browser page has no process. There the working directory is a virtual one in the page's
//! filesystem, `/` at first, and the identifier is a constant.

#[cfg(not(target_arch = "wasm32"))]
use std::{
  io,
  path::{Path, PathBuf},
};

/// The working directory, against which relative paths resolve.
#[cfg(not(target_arch = "wasm32"))]
pub fn current_dir() -> io::Result<PathBuf> {
  std::env::current_dir()
}

/// Changes the working directory.
#[cfg(not(target_arch = "wasm32"))]
pub fn set_current_dir(path: impl AsRef<Path>) -> io::Result<()> {
  std::env::set_current_dir(path)
}

/// An identifier unique to this process, used to name staging files.
#[cfg(not(target_arch = "wasm32"))]
pub fn id() -> u32 {
  std::process::id()
}

#[cfg(target_arch = "wasm32")]
pub use web::{current_dir, id, set_current_dir};

#[cfg(target_arch = "wasm32")]
mod web {
  use std::{
    cell::RefCell,
    io,
    path::{Path, PathBuf},
  };

  use crate::platform::fs;

  thread_local! {
    static CURRENT_DIR: RefCell<PathBuf> = RefCell::new(PathBuf::from("/"));
  }

  /// The virtual working directory, against which relative paths resolve; `/` at first.
  pub fn current_dir() -> io::Result<PathBuf> {
    Ok(CURRENT_DIR.with(|current| current.borrow().clone()))
  }

  /// Changes the virtual working directory to an existing directory.
  pub fn set_current_dir(path: impl AsRef<Path>) -> io::Result<()> {
    let path = fs::canonicalize(path)?;
    if !fs::is_dir(&path) {
      return Err(io::Error::new(io::ErrorKind::NotADirectory, path.display().to_string()));
    }
    CURRENT_DIR.with(|current| *current.borrow_mut() = path);
    Ok(())
  }

  /// A constant: the page runs a single process.
  pub fn id() -> u32 {
    1
  }
}
