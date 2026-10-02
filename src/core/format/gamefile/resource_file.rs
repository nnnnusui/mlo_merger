use std::io::{self, Read};

use flate2::read::DeflateDecoder;

const RSC7_MAGIC: u32 = u32::from_le_bytes(*b"RSC7");
const RSC7_HEADER_SIZE: usize = 16;

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

fn page_data_size(flags: u32) -> io::Result<usize> {
  let page_count = ((flags >> 27) & 0x1)
    + (((flags >> 26) & 0x1) << 1)
    + (((flags >> 25) & 0x1) << 2)
    + (((flags >> 24) & 0x1) << 3)
    + (((flags >> 17) & 0x7f) << 4)
    + (((flags >> 11) & 0x3f) << 5)
    + (((flags >> 7) & 0x0f) << 6)
    + (((flags >> 5) & 0x03) << 7)
    + (((flags >> 4) & 0x01) << 8);
  let page_size = 0x200usize
    .checked_shl(flags & 0x0f)
    .ok_or_else(|| invalid_data("RSC7 page size shift is invalid"))?;
  (page_count as usize)
    .checked_mul(page_size)
    .ok_or_else(|| invalid_data("RSC7 page size overflows address space"))
}

fn read_u32(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u32> {
  let end = offset.checked_add(4).ok_or_else(|| invalid_data("RSC7 header offset overflow"))?;
  let value = bytes.get(offset..end).ok_or_else(|| invalid_data("RSC7 header is truncated"))?;
  Ok(u32::from_le_bytes(value.try_into().expect("four-byte slice")))
}

fn invalid_data(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
  use super::Rsc7Resource;

  #[test]
  fn decodes_checked_in_ymap_resource() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("asset/extracted/brofx_mansion_06___apa_ch2_occl_05.ymap");
    let bytes = std::fs::read(fixture).unwrap();
    let resource = Rsc7Resource::decode(&bytes).unwrap();

    assert_eq!(resource.version, 2);
    assert_eq!(resource.system_data.len(), 0xa000);
    assert!(resource.graphics_data.is_empty());
  }

  #[test]
  fn decodes_all_checked_in_ymap_resources() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/extracted");
    let mut decoded = 0;

    for entry in std::fs::read_dir(fixtures).unwrap() {
      let path = entry.unwrap().path();
      if path.extension().is_some_and(|extension| extension == "ymap") {
        let bytes = std::fs::read(&path).unwrap();
        Rsc7Resource::decode(&bytes).unwrap_or_else(|error| {
          panic!("failed to decode {}: {error}", path.display());
        });
        decoded += 1;
      }
    }

    assert!(decoded > 0);
  }

  #[test]
  fn rejects_non_rsc7_input() {
    let error = Rsc7Resource::decode(&[0; 16]).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
  }

  #[test]
  fn reads_system_and_graphics_virtual_addresses() {
    let resource = Rsc7Resource {
      version: 2,
      system_flags: 0,
      graphics_flags: 0,
      system_data: vec![1, 2, 3],
      graphics_data: vec![4, 5, 6],
    };

    assert_eq!(resource.read_address(0x5000_0001, 2).unwrap(), &[2, 3]);
    assert_eq!(resource.read_address(0x6000_0001, 2).unwrap(), &[5, 6]);
    assert!(resource.read_address(0x5000_0002, 2).is_err());
  }
}
