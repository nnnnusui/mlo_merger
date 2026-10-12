//! Buffered metadata I/O and native YMAP model conversion.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::Result;
use crate::core::format::ymap::binary::ymap_to_model;
use crate::core::format::ymap::model::Ymap;

/// Reads a native RSC7 or PSO YMAP into its typed model.
pub(crate) fn read_ymap(path: &Path) -> Result<Ymap> {
  Ok(ymap_to_model(&fs::read(path)?, &HashMap::new())?)
}

pub(crate) use crate::common::json::write_json;
