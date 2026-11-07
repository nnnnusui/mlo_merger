//! Recursive traversal of the virtual filesystem, mirroring `walkdir` 2.

use std::{
  ffi::{OsStr, OsString},
  fmt, io,
  path::{Path, PathBuf},
};

use super::{FileType, Metadata, metadata, read_dir, symlink_metadata};

/// A builder for a recursive traversal rooted at one path.
#[derive(Debug, Clone)]
pub struct WalkDir {
  root: PathBuf,
  follow_links: bool,
  follow_root_links: bool,
  contents_first: bool,
  min_depth: usize,
  max_depth: usize,
}

impl WalkDir {
  pub fn new(root: impl AsRef<Path>) -> Self {
    Self {
      root: root.as_ref().to_path_buf(),
      follow_links: false,
      follow_root_links: true,
      contents_first: false,
      min_depth: 0,
      max_depth: usize::MAX,
    }
  }

  /// Follows symlinks to directories and reports the type of their targets.
  pub fn follow_links(
    mut self,
    yes: bool,
  ) -> Self {
    self.follow_links = yes;
    self
  }

  /// Follows a symlink given as the root; on by default.
  pub fn follow_root_links(
    mut self,
    yes: bool,
  ) -> Self {
    self.follow_root_links = yes;
    self
  }

  /// Yields a directory after its contents rather than before.
  pub fn contents_first(
    mut self,
    yes: bool,
  ) -> Self {
    self.contents_first = yes;
    self
  }

  pub fn min_depth(
    mut self,
    depth: usize,
  ) -> Self {
    self.min_depth = depth;
    self
  }

  pub fn max_depth(
    mut self,
    depth: usize,
  ) -> Self {
    self.max_depth = depth;
    self
  }

  /// Entries are always sorted by file name, so this changes nothing.
  pub fn sort_by_file_name(self) -> Self {
    self
  }
}

impl IntoIterator for WalkDir {
  type IntoIter = IntoIter;
  type Item = Result<DirEntry, Error>;

  fn into_iter(self) -> IntoIter {
    IntoIter {
      start: Some(self.root.clone()),
      options: self,
      stack: Vec::new(),
    }
  }
}

/// A directory being read: its remaining children, and itself when yielded after them.
struct Frame {
  children: std::vec::IntoIter<Result<DirEntry, Error>>,
  after: Option<DirEntry>,
}

/// The traversal, yielding entries depth first with each directory's children by name.
pub struct IntoIter {
  options: WalkDir,
  start: Option<PathBuf>,
  stack: Vec<Frame>,
}

impl fmt::Debug for IntoIter {
  fn fmt(
    &self,
    formatter: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    formatter.debug_struct("IntoIter").field("options", &self.options).finish_non_exhaustive()
  }
}

impl IntoIter {
  /// Descends into `entry` when it is a directory and decides whether to yield it now.
  fn visit(
    &mut self,
    entry: DirEntry,
  ) -> Option<Result<DirEntry, Error>> {
    let descend = entry.file_type.is_dir() && entry.depth < self.options.max_depth;
    if descend {
      let children = self.children(&entry);
      let after = self.options.contents_first.then(|| entry.clone());
      self.stack.push(Frame {
        children: children.into_iter(),
        after,
      });
      if self.options.contents_first {
        return None;
      }
    }
    (entry.depth >= self.options.min_depth).then_some(Ok(entry))
  }

  fn children(
    &self,
    parent: &DirEntry,
  ) -> Vec<Result<DirEntry, Error>> {
    let depth = parent.depth + 1;
    let entries = match read_dir(&parent.path) {
      Ok(entries) => entries,
      Err(error) => return vec![Err(Error::new(Some(parent.path.clone()), parent.depth, error))],
    };
    entries
      .map(|entry| {
        let entry = entry.map_err(|error| Error::new(Some(parent.path.clone()), depth, error))?;
        DirEntry::new(entry.path(), depth, self.options.follow_links)
      })
      .collect()
  }
}

impl Iterator for IntoIter {
  type Item = Result<DirEntry, Error>;

  fn next(&mut self) -> Option<Self::Item> {
    if let Some(root) = self.start.take() {
      let follow = self.options.follow_links || self.options.follow_root_links;
      match DirEntry::new(root, 0, follow) {
        Ok(entry) => {
          if let Some(item) = self.visit(entry) {
            return Some(item);
          }
        }
        Err(error) => return Some(Err(error)),
      }
    }
    loop {
      let frame = self.stack.last_mut()?;
      match frame.children.next() {
        Some(Ok(entry)) => {
          if let Some(item) = self.visit(entry) {
            return Some(item);
          }
        }
        Some(Err(error)) => return Some(Err(error)),
        None => {
          let after = self.stack.pop().and_then(|frame| frame.after);
          if let Some(entry) = after
            && entry.depth >= self.options.min_depth
          {
            return Some(Ok(entry));
          }
        }
      }
    }
  }
}

/// An entry yielded by the traversal.
#[derive(Debug, Clone)]
pub struct DirEntry {
  path: PathBuf,
  name: OsString,
  file_type: FileType,
  depth: usize,
  follow_link: bool,
  is_symlink: bool,
}

impl DirEntry {
  fn new(
    path: PathBuf,
    depth: usize,
    follow: bool,
  ) -> Result<Self, Error> {
    let error = |error| Error::new(Some(path.clone()), depth, error);
    let own = symlink_metadata(&path).map_err(error)?;
    let is_symlink = own.is_symlink();
    let follow_link = follow && is_symlink;
    let file_type =
      if follow_link { metadata(&path).map_err(error)?.file_type() } else { own.file_type() };
    let name = path.file_name().map_or_else(|| path.as_os_str().to_owned(), OsStr::to_owned);
    Ok(Self {
      path,
      name,
      file_type,
      depth,
      follow_link,
      is_symlink,
    })
  }

  pub fn path(&self) -> &Path {
    &self.path
  }

  pub fn into_path(self) -> PathBuf {
    self.path
  }

  /// Whether the entry is a symlink, even when the traversal follows it.
  pub fn path_is_symlink(&self) -> bool {
    self.is_symlink
  }

  /// The entry's type, of the target when the traversal follows symlinks.
  pub fn file_type(&self) -> FileType {
    self.file_type
  }

  pub fn file_name(&self) -> &OsStr {
    &self.name
  }

  pub fn depth(&self) -> usize {
    self.depth
  }

  pub fn metadata(&self) -> Result<Metadata, Error> {
    let result = if self.follow_link { metadata(&self.path) } else { symlink_metadata(&self.path) };
    result.map_err(|error| Error::new(Some(self.path.clone()), self.depth, error))
  }
}

/// A failure to read an entry during the traversal.
#[derive(Debug)]
pub struct Error {
  path: Option<PathBuf>,
  depth: usize,
  inner: io::Error,
}

impl Error {
  fn new(
    path: Option<PathBuf>,
    depth: usize,
    inner: io::Error,
  ) -> Self {
    Self {
      path,
      depth,
      inner,
    }
  }

  pub fn path(&self) -> Option<&Path> {
    self.path.as_deref()
  }

  pub fn depth(&self) -> usize {
    self.depth
  }

  pub fn io_error(&self) -> Option<&io::Error> {
    Some(&self.inner)
  }

  pub fn into_io_error(self) -> Option<io::Error> {
    Some(self.inner)
  }
}

impl fmt::Display for Error {
  fn fmt(
    &self,
    formatter: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    match &self.path {
      Some(path) => {
        write!(formatter, "IO error for operation on {}: {}", path.display(), self.inner)
      }
      None => self.inner.fmt(formatter),
    }
  }
}

impl std::error::Error for Error {
  fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
    Some(&self.inner)
  }
}

impl From<Error> for io::Error {
  fn from(error: Error) -> Self {
    io::Error::new(error.inner.kind(), error)
  }
}
