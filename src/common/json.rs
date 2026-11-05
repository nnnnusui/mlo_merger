use super::Result;
use serde::{Serialize, de::DeserializeOwned};
use std::{
  fs,
  io::{BufWriter, Write},
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

/// Reads the JSON file at `path`.
///
/// The file is read whole before parsing: serde_json parses a slice several times faster than a
/// reader, which matters for metadata of several megabytes read on every run.
pub(crate) fn read_json<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
  Ok(serde_json::from_slice(&fs::read(path)?)?)
}
