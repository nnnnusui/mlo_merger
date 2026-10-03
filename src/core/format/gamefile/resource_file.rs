use std::io::{self, Read};

use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use std::io::Write;

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
  /// Creates an RSC7 resource from unpadded system and graphics bytes.
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

  /// Encodes the current system and graphics pages as a compressed RSC7 resource.
  pub fn encode(&self) -> io::Result<Vec<u8>> {
    let system_size = page_data_size(self.system_flags)?;
    let graphics_size = page_data_size(self.graphics_flags)?;
    if self.system_data.len() != system_size || self.graphics_data.len() != graphics_size {
      return Err(invalid_data("RSC7 page bytes do not match their encoded flags"));
    }

    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&self.system_data)?;
    encoder.write_all(&self.graphics_data)?;
    let compressed = encoder.finish()?;

    let mut bytes = Vec::with_capacity(RSC7_HEADER_SIZE + compressed.len());
    bytes.extend_from_slice(&RSC7_MAGIC.to_le_bytes());
    bytes.extend_from_slice(&self.version.to_le_bytes());
    bytes.extend_from_slice(&self.system_flags.to_le_bytes());
    bytes.extend_from_slice(&self.graphics_flags.to_le_bytes());
    bytes.extend_from_slice(&compressed);
    Ok(bytes)
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

fn flags_for_data(
  length: usize,
  version: u32,
) -> io::Result<u32> {
  if length == 0 {
    return Ok((version & 0xf) << 28);
  }

  for shift in 4..=15u32 {
    let page_size = 0x200usize
      .checked_shl(shift)
      .ok_or_else(|| invalid_data("RSC7 page size shift is invalid"))?;
    if length <= page_size {
      return Ok(((version & 0xf) << 28) | shift | (1 << 27));
    }
  }
  for (multiplier, count_flag) in [(2usize, 1 << 26), (4, 1 << 25), (8, 1 << 24), (16, 1 << 17)] {
    if length <= (0x200usize << 15) * multiplier && length <= 0x1000_0000 {
      return Ok(((version & 0xf) << 28) | 15 | count_flag);
    }
  }

  Err(invalid_data("RSC7 data exceeds the maximum encodable page count"))
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
  use crate::core::format::gamefile::test_support::{sample_ymap_binary, sample_ymap_xml};

  #[test]
  fn contiguous_resources_use_one_page_without_splitting_blocks() {
    for length in [1, 9_000, 50_000, 3_000_000] {
      let data = vec![0x5a; length];
      let resource = Rsc7Resource::from_pages(2, &data, &[]).unwrap();
      let flags = resource.system_flags;
      let count = ((flags >> 27) & 1)
        + ((flags >> 26) & 1)
        + ((flags >> 25) & 1)
        + ((flags >> 24) & 1)
        + ((flags >> 17) & 0x7f)
        + ((flags >> 11) & 0x3f)
        + ((flags >> 7) & 0xf)
        + ((flags >> 5) & 3)
        + ((flags >> 4) & 1);
      assert_eq!(count, 1);
      assert_eq!(&resource.system_data[..length], data.as_slice());
      assert_eq!(Rsc7Resource::decode(&resource.encode().unwrap()).unwrap(), resource);
    }
  }

  #[test]
  fn decodes_sample_ymap_resource() {
    let xml = sample_ymap_xml("parent_refs/child.ymap.xml");
    let bytes = sample_ymap_binary(&xml);
    let resource = Rsc7Resource::decode(&bytes).unwrap();

    assert_eq!(resource.version, 2);
    assert!(!resource.system_data.is_empty());
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
  fn encode_decode_preserves_all_checked_in_ymap_pages() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/extracted");
    let mut round_tripped = 0;

    for entry in std::fs::read_dir(fixtures).unwrap() {
      let path = entry.unwrap().path();
      if path.extension().is_some_and(|extension| extension == "ymap") {
        let bytes = std::fs::read(&path).unwrap();
        let original = Rsc7Resource::decode(&bytes).unwrap();
        let encoded = original.encode().unwrap();
        let decoded = Rsc7Resource::decode(&encoded).unwrap();
        assert_eq!(decoded, original, "RSC7 page mismatch for {}", path.display());
        round_tripped += 1;
      }
    }

    assert!(round_tripped > 0);
  }

  #[test]
  fn creates_page_flags_for_arbitrary_resource_data() {
    let system = vec![0x5a; 0x2345];
    let graphics = vec![0xa5; 0x401];
    let resource = Rsc7Resource::from_pages(2, &system, &graphics).unwrap();
    let encoded = resource.encode().unwrap();
    let decoded = Rsc7Resource::decode(&encoded).unwrap();

    assert_eq!(decoded.version, 2);
    assert_eq!(decoded.system_data.len(), 0x4000);
    assert_eq!(decoded.graphics_data.len(), 0x2000);
    assert_eq!(&decoded.system_data[..system.len()], system);
    assert_eq!(&decoded.graphics_data[..graphics.len()], graphics);
    assert!(decoded.system_data[system.len()..].iter().all(|byte| *byte == 0));
    assert!(decoded.graphics_data[graphics.len()..].iter().all(|byte| *byte == 0));
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
