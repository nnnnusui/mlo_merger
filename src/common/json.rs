use super::{Result, path::reject_symlinks};
use serde::{Serialize, de::DeserializeOwned};
use std::{
  fs,
  io::{BufReader, BufWriter, Write},
  path::Path,
};

pub(crate) fn write_json(
  path: &Path,
  value: &impl Serialize,
) -> Result<()> {
  let mut writer = BufWriter::new(fs::File::create(path)?);
  serde_json::to_writer_pretty(&mut writer, value)?;
  writer.flush()?;
  Ok(())
}

/// Missing or malformed cache metadata is a cache miss; other I/O errors propagate.
pub(crate) fn read_optional<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
  reject_symlinks(path)?;
  match fs::File::open(path) {
    Ok(file) => Ok(serde_json::from_reader(BufReader::new(file)).ok()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
    Err(error) => Err(error.into()),
  }
}

pub(crate) fn atomic_json(
  path: &Path,
  value: &impl Serialize,
) -> Result<()> {
  reject_symlinks(path)?;
  let temporary = path.with_file_name(format!(
    ".{}.{}-{}.tmp",
    path.file_name().ok_or("Missing JSON filename")?.to_string_lossy(),
    std::process::id(),
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos()
  ));
  let result = (|| {
    write_json(&temporary, value)?;
    fs::rename(&temporary, path)?;
    Ok(())
  })();
  let _ = fs::remove_file(temporary);
  result
}
