//! Shared safe cache-path checks and cross-device file moves.

use super::Result;
use std::{
  fs,
  path::{Component, Path},
};

pub(super) fn ensure_safe_path(
  root: &Path,
  path: &Path,
) -> Result<()> {
  let relative = path.strip_prefix(root)?;
  let mut current = root.to_path_buf();
  for component in relative.components() {
    if !matches!(component, Component::Normal(_)) {
      return Err("Unsafe source cache path".into());
    }
    current.push(component);
    if fs::symlink_metadata(&current).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
      return Err(format!("Refusing symlink cache path: {}", current.display()).into());
    }
  }
  Ok(())
}

pub(super) fn move_file(
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
