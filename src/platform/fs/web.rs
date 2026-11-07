//! The page's virtual filesystem, implemented in `web/vfs.js`.
//!
//! Every item mirrors its `std::fs` namesake, including the `io::ErrorKind` of failures, so
//! library code behaves as it does on disk. Paths are absolute POSIX paths in the virtual
//! filesystem; relative paths resolve against [`process::current_dir`].

mod walk_impl;

use std::{
  ffi::OsString,
  fmt, io,
  path::{Path, PathBuf},
  time::{Duration, SystemTime, UNIX_EPOCH},
};

use wasm_bindgen::prelude::*;

use crate::platform::process;

/// Recursive directory traversal with the subset of the `walkdir` API the library uses.
pub mod walk {
  pub use super::walk_impl::{DirEntry, Error, IntoIter, WalkDir};
}
pub use walk::WalkDir;

#[wasm_bindgen(module = "/web/vfs.js")]
extern "C" {
  #[wasm_bindgen(catch, js_name = readFile)]
  fn js_read_file(path: &str) -> Result<Vec<u8>, JsValue>;
  #[wasm_bindgen(catch, js_name = writeFile)]
  fn js_write_file(
    path: &str,
    data: &[u8],
  ) -> Result<(), JsValue>;
  #[wasm_bindgen(catch, js_name = stat)]
  fn js_stat(
    path: &str,
    follow: bool,
  ) -> Result<Vec<f64>, JsValue>;
  #[wasm_bindgen(catch, js_name = readDir)]
  fn js_read_dir(path: &str) -> Result<Vec<String>, JsValue>;
  #[wasm_bindgen(catch, js_name = mkdir)]
  fn js_mkdir(path: &str) -> Result<(), JsValue>;
  #[wasm_bindgen(catch, js_name = mkdirAll)]
  fn js_mkdir_all(path: &str) -> Result<(), JsValue>;
  #[wasm_bindgen(catch, js_name = unlink)]
  fn js_unlink(path: &str) -> Result<(), JsValue>;
  #[wasm_bindgen(catch, js_name = rmdir)]
  fn js_rmdir(path: &str) -> Result<(), JsValue>;
  #[wasm_bindgen(catch, js_name = rmAll)]
  fn js_rm_all(path: &str) -> Result<(), JsValue>;
  #[wasm_bindgen(catch, js_name = rename)]
  fn js_rename(
    from: &str,
    to: &str,
  ) -> Result<(), JsValue>;
  #[wasm_bindgen(catch, js_name = copyFile)]
  fn js_copy_file(
    from: &str,
    to: &str,
  ) -> Result<f64, JsValue>;
  #[wasm_bindgen(catch, js_name = hardLink)]
  fn js_hard_link(
    from: &str,
    to: &str,
  ) -> Result<(), JsValue>;
  #[wasm_bindgen(catch, js_name = symlink)]
  fn js_symlink(
    target: &str,
    link: &str,
  ) -> Result<(), JsValue>;
  #[wasm_bindgen(catch, js_name = readLink)]
  fn js_read_link(path: &str) -> Result<String, JsValue>;
  #[wasm_bindgen(catch, js_name = realpath)]
  fn js_realpath(path: &str) -> Result<String, JsValue>;

  #[wasm_bindgen(catch, js_name = open)]
  fn js_open(
    path: &str,
    flags: u32,
  ) -> Result<u32, JsValue>;
  #[wasm_bindgen(catch, js_name = fdRead)]
  fn js_fd_read(
    fd: u32,
    position: f64,
    length: u32,
  ) -> Result<Vec<u8>, JsValue>;
  #[wasm_bindgen(catch, js_name = fdWrite)]
  fn js_fd_write(
    fd: u32,
    position: f64,
    data: &[u8],
  ) -> Result<(), JsValue>;
  #[wasm_bindgen(catch, js_name = fdStat)]
  fn js_fd_stat(fd: u32) -> Result<Vec<f64>, JsValue>;
  #[wasm_bindgen(catch, js_name = fdSetModified)]
  fn js_fd_set_modified(
    fd: u32,
    milliseconds: f64,
  ) -> Result<(), JsValue>;
  #[wasm_bindgen(js_name = close)]
  fn js_close(fd: u32);
}

// `open` flags, matching `web/vfs.js`.
const READ: u32 = 1;
const WRITE: u32 = 2;
const APPEND: u32 = 4;
const TRUNCATE: u32 = 8;
const CREATE: u32 = 16;
const CREATE_NEW: u32 = 32;

/// Converts an error thrown by `web/vfs.js`, whose `code` names a POSIX error.
fn io_error(error: JsValue) -> io::Error {
  let field = |name: &str| {
    js_sys::Reflect::get(&error, &JsValue::from_str(name)).ok().and_then(|value| value.as_string())
  };
  let kind = match field("code").as_deref() {
    Some("ENOENT") => io::ErrorKind::NotFound,
    Some("EEXIST") => io::ErrorKind::AlreadyExists,
    Some("ENOTDIR") => io::ErrorKind::NotADirectory,
    Some("EISDIR") => io::ErrorKind::IsADirectory,
    Some("ENOTEMPTY") => io::ErrorKind::DirectoryNotEmpty,
    Some("EXDEV") => io::ErrorKind::CrossesDevices,
    Some("EINVAL") => io::ErrorKind::InvalidInput,
    Some("EBADF") | Some("EACCES") | Some("EPERM") => io::ErrorKind::PermissionDenied,
    Some("EBUSY") => io::ErrorKind::ResourceBusy,
    _ => io::ErrorKind::Other,
  };
  let message = field("message").unwrap_or_else(|| format!("{error:?}"));
  io::Error::new(kind, message)
}

fn call<T>(result: Result<T, JsValue>) -> io::Result<T> {
  result.map_err(io_error)
}

/// The absolute virtual path of `path` as a string for `web/vfs.js`.
fn resolve(path: &Path) -> io::Result<String> {
  let absolute =
    if path.is_absolute() { path.to_path_buf() } else { process::current_dir()?.join(path) };
  absolute.into_os_string().into_string().map_err(|path| {
    io::Error::new(io::ErrorKind::InvalidInput, format!("Path is not UTF-8: {path:?}"))
  })
}

fn milliseconds(time: SystemTime) -> f64 {
  match time.duration_since(UNIX_EPOCH) {
    Ok(duration) => duration.as_secs_f64() * 1000.0,
    Err(error) => -error.duration().as_secs_f64() * 1000.0,
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
  File,
  Dir,
  Symlink,
}

/// The kind of a filesystem entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileType(Kind);

impl FileType {
  pub fn is_file(&self) -> bool {
    self.0 == Kind::File
  }

  pub fn is_dir(&self) -> bool {
    self.0 == Kind::Dir
  }

  pub fn is_symlink(&self) -> bool {
    self.0 == Kind::Symlink
  }
}

/// What `stat` reports about an entry.
#[derive(Debug, Clone)]
pub struct Metadata {
  file_type: FileType,
  len: u64,
  modified: SystemTime,
}

impl Metadata {
  /// Parses `[kind, length, modified milliseconds]` from `web/vfs.js`.
  fn from_stat(stat: Vec<f64>) -> io::Result<Self> {
    let [kind, len, modified] = stat[..] else {
      return Err(io::Error::other("stat returned a malformed result"));
    };
    let kind = match kind as u8 {
      0 => Kind::File,
      1 => Kind::Dir,
      2 => Kind::Symlink,
      _ => return Err(io::Error::other("stat returned an unknown kind")),
    };
    let modified = if modified >= 0.0 {
      UNIX_EPOCH + Duration::from_secs_f64(modified / 1000.0)
    } else {
      UNIX_EPOCH - Duration::from_secs_f64(-modified / 1000.0)
    };
    Ok(Self {
      file_type: FileType(kind),
      len: len as u64,
      modified,
    })
  }

  pub fn file_type(&self) -> FileType {
    self.file_type
  }

  pub fn is_file(&self) -> bool {
    self.file_type.is_file()
  }

  pub fn is_dir(&self) -> bool {
    self.file_type.is_dir()
  }

  pub fn is_symlink(&self) -> bool {
    self.file_type.is_symlink()
  }

  #[allow(clippy::len_without_is_empty, reason = "mirrors std::fs::Metadata")]
  pub fn len(&self) -> u64 {
    self.len
  }

  pub fn modified(&self) -> io::Result<SystemTime> {
    Ok(self.modified)
  }
}

/// Timestamps to apply with [`File::set_times`]; only the modification time is kept.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileTimes {
  modified: Option<SystemTime>,
}

impl FileTimes {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn set_modified(
    mut self,
    time: SystemTime,
  ) -> Self {
    self.modified = Some(time);
    self
  }

  pub fn set_accessed(
    self,
    _time: SystemTime,
  ) -> Self {
    self
  }
}

/// An open file. Like an operating system handle, it follows the file through renames.
pub struct File {
  fd: u32,
  position: u64,
  append: bool,
}

impl fmt::Debug for File {
  fn fmt(
    &self,
    formatter: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    formatter.debug_struct("File").field("fd", &self.fd).finish()
  }
}

impl File {
  pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
    OpenOptions::new().read(true).open(path)
  }

  pub fn create(path: impl AsRef<Path>) -> io::Result<Self> {
    OpenOptions::new().write(true).create(true).truncate(true).open(path)
  }

  pub fn create_new(path: impl AsRef<Path>) -> io::Result<Self> {
    OpenOptions::new().read(true).write(true).create_new(true).open(path)
  }

  pub fn options() -> OpenOptions {
    OpenOptions::new()
  }

  pub fn metadata(&self) -> io::Result<Metadata> {
    Metadata::from_stat(call(js_fd_stat(self.fd))?)
  }

  pub fn set_times(
    &self,
    times: FileTimes,
  ) -> io::Result<()> {
    match times.modified {
      Some(time) => call(js_fd_set_modified(self.fd, milliseconds(time))),
      None => Ok(()),
    }
  }

  pub fn set_modified(
    &self,
    time: SystemTime,
  ) -> io::Result<()> {
    self.set_times(FileTimes::new().set_modified(time))
  }

  /// Writes go straight to the virtual filesystem, so there is nothing to sync.
  pub fn sync_all(&self) -> io::Result<()> {
    Ok(())
  }
}

impl Drop for File {
  fn drop(&mut self) {
    js_close(self.fd);
  }
}

impl io::Read for File {
  fn read(
    &mut self,
    buffer: &mut [u8],
  ) -> io::Result<usize> {
    let length = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
    let data = call(js_fd_read(self.fd, self.position as f64, length))?;
    buffer[..data.len()].copy_from_slice(&data);
    self.position += data.len() as u64;
    Ok(data.len())
  }
}

impl io::Write for File {
  fn write(
    &mut self,
    data: &[u8],
  ) -> io::Result<usize> {
    if self.append {
      self.position = self.metadata()?.len();
    }
    call(js_fd_write(self.fd, self.position as f64, data))?;
    self.position += data.len() as u64;
    Ok(data.len())
  }

  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

impl io::Seek for File {
  fn seek(
    &mut self,
    position: io::SeekFrom,
  ) -> io::Result<u64> {
    let target = match position {
      io::SeekFrom::Start(offset) => Some(offset),
      io::SeekFrom::End(offset) => self.metadata()?.len().checked_add_signed(offset),
      io::SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
    };
    self.position = target.ok_or_else(|| {
      io::Error::new(io::ErrorKind::InvalidInput, "Seek to a negative or overflowing position")
    })?;
    Ok(self.position)
  }
}

/// How to open a [`File`], as `std::fs::OpenOptions`.
#[derive(Debug, Clone, Default)]
pub struct OpenOptions {
  flags: u32,
}

impl OpenOptions {
  pub fn new() -> Self {
    Self::default()
  }

  fn flag(
    &mut self,
    flag: u32,
    enabled: bool,
  ) -> &mut Self {
    if enabled {
      self.flags |= flag;
    } else {
      self.flags &= !flag;
    }
    self
  }

  pub fn read(
    &mut self,
    enabled: bool,
  ) -> &mut Self {
    self.flag(READ, enabled)
  }

  pub fn write(
    &mut self,
    enabled: bool,
  ) -> &mut Self {
    self.flag(WRITE, enabled)
  }

  pub fn append(
    &mut self,
    enabled: bool,
  ) -> &mut Self {
    self.flag(APPEND, enabled)
  }

  pub fn truncate(
    &mut self,
    enabled: bool,
  ) -> &mut Self {
    self.flag(TRUNCATE, enabled)
  }

  pub fn create(
    &mut self,
    enabled: bool,
  ) -> &mut Self {
    self.flag(CREATE, enabled)
  }

  pub fn create_new(
    &mut self,
    enabled: bool,
  ) -> &mut Self {
    self.flag(CREATE_NEW, enabled)
  }

  pub fn open(
    &self,
    path: impl AsRef<Path>,
  ) -> io::Result<File> {
    let fd = call(js_open(&resolve(path.as_ref())?, self.flags))?;
    Ok(File {
      fd,
      position: 0,
      append: self.flags & APPEND != 0,
    })
  }
}

/// An entry yielded by [`read_dir`].
#[derive(Debug, Clone)]
pub struct DirEntry {
  path: PathBuf,
  name: String,
}

impl DirEntry {
  /// The directory passed to [`read_dir`] joined with the entry's name.
  pub fn path(&self) -> PathBuf {
    self.path.clone()
  }

  pub fn file_name(&self) -> OsString {
    OsString::from(&self.name)
  }

  /// The entry's own metadata; a symlink is not followed.
  pub fn metadata(&self) -> io::Result<Metadata> {
    symlink_metadata(&self.path)
  }

  /// The entry's own type; a symlink is not followed.
  pub fn file_type(&self) -> io::Result<FileType> {
    Ok(self.metadata()?.file_type())
  }
}

/// The entries of a directory, sorted by name.
#[derive(Debug)]
pub struct ReadDir {
  entries: std::vec::IntoIter<DirEntry>,
}

impl Iterator for ReadDir {
  type Item = io::Result<DirEntry>;

  fn next(&mut self) -> Option<Self::Item> {
    self.entries.next().map(Ok)
  }
}

pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
  call(js_read_file(&resolve(path.as_ref())?))
}

pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
  String::from_utf8(read(path)?)
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.utf8_error()))
}

pub fn write(
  path: impl AsRef<Path>,
  contents: impl AsRef<[u8]>,
) -> io::Result<()> {
  call(js_write_file(&resolve(path.as_ref())?, contents.as_ref()))
}

pub fn create_dir(path: impl AsRef<Path>) -> io::Result<()> {
  call(js_mkdir(&resolve(path.as_ref())?))
}

pub fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
  call(js_mkdir_all(&resolve(path.as_ref())?))
}

pub fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
  call(js_unlink(&resolve(path.as_ref())?))
}

pub fn remove_dir(path: impl AsRef<Path>) -> io::Result<()> {
  call(js_rmdir(&resolve(path.as_ref())?))
}

pub fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
  call(js_rm_all(&resolve(path.as_ref())?))
}

pub fn rename(
  from: impl AsRef<Path>,
  to: impl AsRef<Path>,
) -> io::Result<()> {
  call(js_rename(&resolve(from.as_ref())?, &resolve(to.as_ref())?))
}

/// Copies a file's contents and returns its length; the copy gets a new modification time.
pub fn copy(
  from: impl AsRef<Path>,
  to: impl AsRef<Path>,
) -> io::Result<u64> {
  Ok(call(js_copy_file(&resolve(from.as_ref())?, &resolve(to.as_ref())?))? as u64)
}

pub fn hard_link(
  original: impl AsRef<Path>,
  link: impl AsRef<Path>,
) -> io::Result<()> {
  call(js_hard_link(&resolve(original.as_ref())?, &resolve(link.as_ref())?))
}

/// Creates a symlink at `link` pointing to the file `target`, kept as written.
pub fn symlink_file(
  target: impl AsRef<Path>,
  link: impl AsRef<Path>,
) -> io::Result<()> {
  let target = target
    .as_ref()
    .to_str()
    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Symlink target is not UTF-8"))?;
  call(js_symlink(target, &resolve(link.as_ref())?))
}

pub fn read_link(path: impl AsRef<Path>) -> io::Result<PathBuf> {
  call(js_read_link(&resolve(path.as_ref())?)).map(PathBuf::from)
}

pub fn canonicalize(path: impl AsRef<Path>) -> io::Result<PathBuf> {
  call(js_realpath(&resolve(path.as_ref())?)).map(PathBuf::from)
}

/// Metadata of `path`, following symlinks.
pub fn metadata(path: impl AsRef<Path>) -> io::Result<Metadata> {
  Metadata::from_stat(call(js_stat(&resolve(path.as_ref())?, true))?)
}

/// Metadata of `path` itself, not following a final symlink.
pub fn symlink_metadata(path: impl AsRef<Path>) -> io::Result<Metadata> {
  Metadata::from_stat(call(js_stat(&resolve(path.as_ref())?, false))?)
}

pub fn read_dir(path: impl AsRef<Path>) -> io::Result<ReadDir> {
  let path = path.as_ref();
  let mut names = call(js_read_dir(&resolve(path)?))?;
  names.sort();
  let entries = names
    .into_iter()
    .map(|name| DirEntry {
      path: path.join(&name),
      name,
    })
    .collect::<Vec<_>>();
  Ok(ReadDir {
    entries: entries.into_iter(),
  })
}

/// Whether `path` exists, following symlinks, as `std::fs::exists`.
pub fn exists(path: impl AsRef<Path>) -> io::Result<bool> {
  match metadata(path) {
    Ok(_) => Ok(true),
    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
    Err(error) => Err(error),
  }
}

/// Whether `path` is an existing file, following symlinks, like [`Path::is_file`].
pub fn is_file(path: impl AsRef<Path>) -> bool {
  metadata(path).is_ok_and(|metadata| metadata.is_file())
}

/// Whether `path` is an existing directory, following symlinks, like [`Path::is_dir`].
pub fn is_dir(path: impl AsRef<Path>) -> bool {
  metadata(path).is_ok_and(|metadata| metadata.is_dir())
}
