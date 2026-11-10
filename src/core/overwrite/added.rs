//! The base source of a YMAP that several resources add.

use crate::common::Result;
use crate::platform::fs;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The extension of an added YMAP overwrite document, after the YMAP's own name.
const DOCUMENT_EXTENSION: &str = ".json";

/// The base source of one YMAP that several resources add, read from
/// `added/ymap/<file name>.json`.
///
/// Without one, the merge takes the YMAP of the resource whose name sorts last, as the one
/// loaded last, and warns. With one, the base source's YMAP takes the place of vanilla: the
/// overwrite cache takes every other resource's differences from it, which are merged into it
/// as resources' changes to vanilla are, and which resource overwrites can leave out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct AddedYmapOverwrite {
  /// Version of the document layout.
  pub schema_version: u32,
  /// The name of the resource (its directory name) whose YMAP is the base.
  pub base_source: String,
}

impl AddedYmapOverwrite {
  /// The current [`Self::schema_version`].
  pub const SCHEMA_VERSION: u32 = 1;
}

/// Reads `<directory>/added/ymap/*.json`, keyed by the lowercase YMAP file name.
pub(super) fn load(directory: &Path) -> Result<BTreeMap<String, AddedYmapOverwrite>> {
  let ymap_dir = directory.join("added").join("ymap");
  let mut overwrites = BTreeMap::new();
  if !fs::is_dir(&ymap_dir) {
    return Ok(overwrites);
  }
  for entry in fs::read_dir(&ymap_dir)? {
    let path = entry?.path();
    let name = path
      .file_name()
      .and_then(|name| name.to_str())
      .ok_or_else(|| format!("Overwrite file name is not UTF-8: {}", path.display()))?
      .to_ascii_lowercase();
    let Some(stream_name) = name.strip_suffix(DOCUMENT_EXTENSION).filter(|name| {
      fs::is_file(&path) && Path::new(name).extension().is_some_and(|extension| extension == "ymap")
    }) else {
      tracing::warn!("Not an added YMAP overwrite; ignoring {}", path.display());
      continue;
    };
    let overwrite: AddedYmapOverwrite = crate::common::json::read_json(&path)
      .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    if overwrite.schema_version != AddedYmapOverwrite::SCHEMA_VERSION {
      return Err(
        format!(
          "{} has schema version {}; expected {}",
          path.display(),
          overwrite.schema_version,
          AddedYmapOverwrite::SCHEMA_VERSION
        )
        .into(),
      );
    }
    overwrites.insert(stream_name.to_string(), overwrite);
  }
  Ok(overwrites)
}
