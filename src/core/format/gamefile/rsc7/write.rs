use std::io;

use super::helpers::{invalid_data, page_data_size};
use super::{RSC7_HEADER_SIZE, RSC7_MAGIC, Rsc7Resource};

impl Rsc7Resource {
  /// Encodes the current system and graphics pages as a compressed RSC7 resource.
  pub fn encode(&self) -> io::Result<Vec<u8>> {
    let system_size = page_data_size(self.system_flags)?;
    let graphics_size = page_data_size(self.graphics_flags)?;
    if self.system_data.len() != system_size || self.graphics_data.len() != graphics_size {
      return Err(invalid_data("RSC7 page bytes do not match their encoded flags"));
    }

    // Compressed with miniz_oxide at flate2's default level, as before flate2 decompressed
    // through zlib-rs, so encoded resources keep the bytes they had; zlib-rs compresses the
    // same data to other, equally valid bytes.
    let mut pages = Vec::with_capacity(system_size + graphics_size);
    pages.extend_from_slice(&self.system_data);
    pages.extend_from_slice(&self.graphics_data);
    let compressed = miniz_oxide::deflate::compress_to_vec(&pages, 6);

    let mut bytes = Vec::with_capacity(RSC7_HEADER_SIZE + compressed.len());
    bytes.extend_from_slice(&RSC7_MAGIC.to_le_bytes());
    bytes.extend_from_slice(&self.version.to_le_bytes());
    bytes.extend_from_slice(&self.system_flags.to_le_bytes());
    bytes.extend_from_slice(&self.graphics_flags.to_le_bytes());
    bytes.extend_from_slice(&compressed);
    Ok(bytes)
  }
}
