//! Reads stream files recorded by the source cache.

use super::Result;
use crate::common::filesystem::read_bytes;
use std::path::Path;

pub(crate) fn read_stream_bytes(path: &Path) -> Result<Vec<u8>> {
  read_bytes(path)
}
