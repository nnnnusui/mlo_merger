//! Light identity helpers; stream filesystem operations belong to source-cache.

use super::Result;
use crate::core::format::ymap::diff::reference_hash;
use std::path::Path;

pub(crate) use crate::core::source_cache::StreamFile as ResourceFile;
#[cfg(test)]
pub(crate) use crate::core::source_cache::discover_stream_inventory as inventory;
pub(super) use crate::core::source_cache::stream_inventory_for as inventory_for;

pub(crate) fn file_hash(path: &str) -> Result<u32> {
  Ok(reference_hash(
    Path::new(path).file_stem().and_then(|name| name.to_str()).ok_or("Invalid YMAP name")?,
  ))
}
