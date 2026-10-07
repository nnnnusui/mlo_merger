//! Buffered metadata I/O and native YMAP model conversion.

use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;

use super::Result;
use crate::core::format::{gamefile::meta_xml::ymap_to_model, ymap::model::Ymap};

/// Reads a native YMAP directly into its typed model without serializing XML.
pub(crate) fn read_ymap(path: &Path) -> Result<Ymap> {
  Ok(ymap_to_model(&fs::read(path)?, &HashMap::new())?)
}

/// Writes JSON with buffered I/O and explicit error propagation on flush.
pub(crate) fn write_json(
  path: &Path,
  value: &impl Serialize,
) -> Result<()> {
  let mut writer = BufWriter::new(fs::File::create(path)?);
  serde_json::to_writer_pretty(&mut writer, value)?;
  writer.flush()?;
  Ok(())
}
