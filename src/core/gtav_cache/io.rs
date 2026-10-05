//! Buffered metadata I/O and native YMAP model conversion.

use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;

use super::Result;
use crate::core::format::{
  gamefile::resource_convert::{NativeResourceFormat, resource_to_xml},
  ymap::{model::Ymap, xml::XmlYmap},
};

/// Reads a native YMAP through the existing native XML/model adapters.
pub(crate) fn read_ymap(path: &Path) -> Result<Ymap> {
  let xml = resource_to_xml(NativeResourceFormat::Ymap, &fs::read(path)?, &HashMap::new())?;
  let xml: XmlYmap = quick_xml::de::from_str(&xml)?;
  Ok(xml.into())
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
