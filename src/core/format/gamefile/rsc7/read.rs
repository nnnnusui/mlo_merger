use std::io::{self, Read};

use flate2::read::DeflateDecoder;

use super::helpers::{invalid_data, page_data_size, read_u32};
use super::{RSC7_HEADER_SIZE, RSC7_MAGIC, Rsc7Resource};

impl Rsc7Resource {
  /// Decodes an OpenIV-compatible RSC7 resource file.
  pub fn decode(bytes: &[u8]) -> io::Result<Self> {
    if bytes.len() < RSC7_HEADER_SIZE {
      return Err(invalid_data("RSC7 header is truncated"));
    }

    let magic = read_u32(bytes, 0)?;
    if magic != RSC7_MAGIC {
      return Err(invalid_data("invalid RSC7 magic"));
    }

    let version = read_u32(bytes, 4)?;
    let system_flags = read_u32(bytes, 8)?;
    let graphics_flags = read_u32(bytes, 12)?;
    let system_size = page_data_size(system_flags)?;
    let graphics_size = page_data_size(graphics_flags)?;
    let decompressed_size = system_size
      .checked_add(graphics_size)
      .ok_or_else(|| invalid_data("RSC7 page sizes overflow"))?;

    let decoder = DeflateDecoder::new(&bytes[RSC7_HEADER_SIZE..]);
    let mut decoder = decoder.take(decompressed_size as u64 + 1);
    let mut decompressed = Vec::with_capacity(decompressed_size);
    decoder.read_to_end(&mut decompressed)?;
    if decompressed.len() != decompressed_size {
      return Err(invalid_data(&format!(
        "RSC7 page sizes require {decompressed_size} decompressed bytes, got {}",
        decompressed.len()
      )));
    }

    let graphics_data = decompressed.split_off(system_size);
    Ok(Self {
      version,
      system_flags,
      graphics_flags,
      system_data: decompressed,
      graphics_data,
    })
  }
}
