use super::Result;
use serde::Serialize;
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
