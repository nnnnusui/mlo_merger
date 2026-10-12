use super::{Result, path::reject_symlinks};
use std::{fs, path::Path};

pub(crate) fn read_bytes(path: &Path) -> Result<Vec<u8>> {
  reject_symlinks(path)?;
  Ok(fs::read(path)?)
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
