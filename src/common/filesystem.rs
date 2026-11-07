use super::{Result, path::reject_symlinks};
use crate::platform::fs;
use std::path::Path;

pub(crate) fn read_bytes(path: &Path) -> Result<Vec<u8>> {
  reject_symlinks(path)?;
  Ok(fs::read(path)?)
}

/// Removes the directories above `path` that are empty, nearest first, up to but not including
/// `stop`, and stops at the first one that is not empty.
pub(crate) fn remove_empty_parents(
  path: &Path,
  stop: &Path,
) -> Result<()> {
  for parent in path.ancestors().skip(1) {
    if parent == stop || !parent.starts_with(stop) {
      break;
    }
    match fs::remove_dir(parent) {
      Ok(()) => {}
      Err(error)
        if matches!(
          error.kind(),
          std::io::ErrorKind::DirectoryNotEmpty | std::io::ErrorKind::NotFound
        ) =>
      {
        break;
      }
      Err(error) => return Err(error.into()),
    }
  }
  Ok(())
}

pub(crate) fn move_file(
  source: &Path,
  destination: &Path,
) -> Result<()> {
  match fs::rename(source, destination) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => {
      let metadata = fs::metadata(source)?;
      fs::copy(source, destination)?;
      fs::File::open(destination)?
        .set_times(fs::FileTimes::new().set_modified(metadata.modified()?))?;
      fs::remove_file(source)?;
      Ok(())
    }
    Err(error) => Err(error.into()),
  }
}
