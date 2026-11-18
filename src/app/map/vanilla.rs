use std::{collections::HashMap, path::Path};

use crate::core::format::ymap::binary::ymap_to_model;
use crate::platform::fs;

/// Reads a native YMAP file into the map model.
pub(crate) fn read_ymap(
  path: &Path
) -> Result<crate::core::format::ymap::model::ymap::Ymap, Box<dyn std::error::Error>> {
  Ok(ymap_to_model(&fs::read(path)?, &HashMap::new())?)
}
