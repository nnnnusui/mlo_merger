use std::io;

use super::helpers::{flags_for_data, invalid_data, page_data_size};

/// A decoded RSC7 resource envelope containing system and graphics pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rsc7Resource {
  /// Resource format version stored in the RSC7 header.
  pub version: u32,
  /// Encoded system-page layout and version flags.
  pub system_flags: u32,
  /// Encoded graphics-page layout and version flags.
  pub graphics_flags: u32,
  /// Decompressed system-page bytes.
  pub system_data: Vec<u8>,
  /// Decompressed graphics-page bytes.
  pub graphics_data: Vec<u8>,
}

impl Rsc7Resource {
  /// Allocates one sufficiently large page per region so contiguous META blocks
  /// cannot straddle page boundaries. Padding is preferred over invalid runtime layout.
  pub fn from_pages(
    version: u32,
    system_data: &[u8],
    graphics_data: &[u8],
  ) -> io::Result<Self> {
    let system_flags = flags_for_data(system_data.len(), (version >> 4) & 0xf)?;
    let graphics_flags = flags_for_data(graphics_data.len(), version & 0xf)?;
    let system_size = page_data_size(system_flags)?;
    let graphics_size = page_data_size(graphics_flags)?;
    let mut padded_system = vec![0; system_size];
    let mut padded_graphics = vec![0; graphics_size];
    padded_system[..system_data.len()].copy_from_slice(system_data);
    padded_graphics[..graphics_data.len()].copy_from_slice(graphics_data);

    Ok(Self {
      version,
      system_flags,
      graphics_flags,
      system_data: padded_system,
      graphics_data: padded_graphics,
    })
  }

  /// Reads a byte range from a resource virtual address.
  pub fn read_address(
    &self,
    address: u64,
    length: usize,
  ) -> io::Result<&[u8]> {
    let address =
      u32::try_from(address).map_err(|_| invalid_data("resource address is invalid"))?;
    let (data, offset) = match address & 0xf000_0000 {
      0x5000_0000 => (&self.system_data, (address & 0x0fff_ffff) as usize),
      0x6000_0000 => (&self.graphics_data, (address & 0x0fff_ffff) as usize),
      _ => return Err(invalid_data("resource address uses an unknown page region")),
    };
    let end =
      offset.checked_add(length).ok_or_else(|| invalid_data("resource address range overflows"))?;
    data
      .get(offset..end)
      .ok_or_else(|| invalid_data("resource address range is outside its page region"))
  }
}
