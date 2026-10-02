use std::io;

use super::resource_file::Rsc7Resource;

const META_ROOT_ADDRESS: u64 = 0x5000_0000;
const META_ROOT_SIZE: usize = 0x80;
const MAX_STRING_LENGTH: usize = 1 << 16;

/// META tables and data blocks embedded in an RSC7 resource.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaResource {
  /// One-based index of the root data block.
  pub root_block_index: i32,
  /// META structure schemas stored in the resource.
  pub structures: Vec<MetaStructureInfo>,
  /// META enum schemas stored in the resource.
  pub enums: Vec<MetaEnumInfo>,
  /// Raw data blocks referenced by the schemas.
  pub data_blocks: Vec<MetaDataBlock>,
  /// Optional META name string.
  pub name: Option<String>,
}

impl MetaResource {
  /// Returns this resource's embedded string names indexed by their Jenk hashes.
  pub fn hash_names(&self) -> std::collections::HashMap<u32, String> {
    let mut names = std::collections::HashMap::new();
    let string_hash = jenk_hash("STRING");
    for block in self.data_blocks.iter().filter(|block| block.structure_name_hash == string_hash) {
      for bytes in block.data.split(|byte| *byte == 0).filter(|bytes| !bytes.is_empty()) {
        if bytes.is_ascii() {
          let name = String::from_utf8_lossy(bytes).into_owned();
          names.entry(jenk_hash(&name)).or_insert(name);
        }
      }
    }
    names
  }
}

/// Schema for one META structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaStructureInfo {
  /// Hash identifying this structure type.
  pub name_hash: u32,
  /// Byte length of one structure instance.
  pub structure_size: u32,
  /// Fields in declaration order.
  pub entries: Vec<MetaStructureEntry>,
}

/// One field in a META structure schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaStructureEntry {
  /// Hash identifying the field name.
  pub name_hash: u32,
  /// Byte offset within its containing structure.
  pub data_offset: u32,
  /// META field data type identifier.
  pub data_type: u8,
  /// Additional type-specific field metadata.
  pub unknown: u8,
  /// Index into a related type table, when used by the field type.
  pub reference_type_index: i16,
  /// Related structure or enum hash, when used by the field type.
  pub reference_key: u32,
}

/// Schema for one META enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaEnumInfo {
  /// Hash identifying this enum type.
  pub name_hash: u32,
  /// Enum values in declaration order.
  pub entries: Vec<MetaEnumEntry>,
}

/// One named value in a META enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaEnumEntry {
  /// Hash identifying the enum value name.
  pub name_hash: u32,
  /// Integer value associated with the name.
  pub value: i32,
}

/// Raw bytes for a META data block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaDataBlock {
  /// Hash identifying the structure represented by this block.
  pub structure_name_hash: u32,
  /// Block payload bytes.
  pub data: Vec<u8>,
}

impl MetaResource {
  /// Reads the META root and its referenced tables from an RSC7 resource.
  pub fn parse(resource: &Rsc7Resource) -> io::Result<Self> {
    let root = resource.read_address(META_ROOT_ADDRESS, META_ROOT_SIZE)?;
    let root_block_index = read_i32(root, 0x1c)?;
    let structures_pointer = read_u64(root, 0x20)?;
    let enums_pointer = read_u64(root, 0x28)?;
    let data_blocks_pointer = read_u64(root, 0x30)?;
    let name_pointer = read_u64(root, 0x38)?;
    let structure_count = read_count(root, 0x48)?;
    let enum_count = read_count(root, 0x4a)?;
    let data_block_count = read_count(root, 0x4c)?;

    let structures = read_structures(resource, structures_pointer, structure_count)?;
    let enums = read_enums(resource, enums_pointer, enum_count)?;
    let data_blocks = read_data_blocks(resource, data_blocks_pointer, data_block_count)?;
    let name = read_optional_string(resource, name_pointer)?;

    if root_block_index < 0 || root_block_index as usize > data_blocks.len() {
      return Err(invalid_data("META root block index is out of range"));
    }

    Ok(Self {
      root_block_index,
      structures,
      enums,
      data_blocks,
      name,
    })
  }
}

fn read_structures(
  resource: &Rsc7Resource,
  pointer: u64,
  count: usize,
) -> io::Result<Vec<MetaStructureInfo>> {
  let records = read_records(resource, pointer, count, 32)?;
  records
    .chunks_exact(32)
    .map(|record| {
      let entry_count = read_i16(record, 30)?;
      if entry_count < 0 {
        return Err(invalid_data("META structure has a negative field count"));
      }
      let entries_pointer = read_u64(record, 16)?;
      let entries = read_structure_entries(resource, entries_pointer, entry_count as usize)?;
      Ok(MetaStructureInfo {
        name_hash: read_u32(record, 0)?,
        structure_size: read_u32(record, 24)?,
        entries,
      })
    })
    .collect()
}

fn read_structure_entries(
  resource: &Rsc7Resource,
  pointer: u64,
  count: usize,
) -> io::Result<Vec<MetaStructureEntry>> {
  let records = read_records(resource, pointer, count, 16)?;
  records
    .chunks_exact(16)
    .map(|record| {
      Ok(MetaStructureEntry {
        name_hash: read_u32(record, 0)?,
        data_offset: read_u32(record, 4)?,
        data_type: record[8],
        unknown: record[9],
        reference_type_index: read_i16(record, 10)?,
        reference_key: read_u32(record, 12)?,
      })
    })
    .collect()
}

fn read_enums(
  resource: &Rsc7Resource,
  pointer: u64,
  count: usize,
) -> io::Result<Vec<MetaEnumInfo>> {
  let records = read_records(resource, pointer, count, 24)?;
  records
    .chunks_exact(24)
    .map(|record| {
      let entries_count = read_i32(record, 16)?;
      if entries_count < 0 {
        return Err(invalid_data("META enum has a negative value count"));
      }
      let entries_pointer = read_u64(record, 8)?;
      let entries = read_enum_entries(resource, entries_pointer, entries_count as usize)?;
      Ok(MetaEnumInfo {
        name_hash: read_u32(record, 0)?,
        entries,
      })
    })
    .collect()
}

fn read_enum_entries(
  resource: &Rsc7Resource,
  pointer: u64,
  count: usize,
) -> io::Result<Vec<MetaEnumEntry>> {
  let records = read_records(resource, pointer, count, 8)?;
  records
    .chunks_exact(8)
    .map(|record| {
      Ok(MetaEnumEntry {
        name_hash: read_u32(record, 0)?,
        value: read_i32(record, 4)?,
      })
    })
    .collect()
}

fn read_data_blocks(
  resource: &Rsc7Resource,
  pointer: u64,
  count: usize,
) -> io::Result<Vec<MetaDataBlock>> {
  let records = read_records(resource, pointer, count, 16)?;
  records
    .chunks_exact(16)
    .map(|record| {
      let data_length = read_i32(record, 4)?;
      if data_length < 0 {
        return Err(invalid_data("META data block has a negative size"));
      }
      let data_pointer = read_u64(record, 8)?;
      let data = if data_length == 0 {
        Vec::new()
      } else {
        resource.read_address(data_pointer, data_length as usize)?.to_vec()
      };
      Ok(MetaDataBlock {
        structure_name_hash: read_u32(record, 0)?,
        data,
      })
    })
    .collect()
}

fn read_records(
  resource: &Rsc7Resource,
  pointer: u64,
  count: usize,
  record_size: usize,
) -> io::Result<&[u8]> {
  let length =
    count.checked_mul(record_size).ok_or_else(|| invalid_data("META table size overflows"))?;
  if length == 0 {
    return Ok(&[]);
  }
  resource.read_address(pointer, length)
}

fn read_optional_string(
  resource: &Rsc7Resource,
  pointer: u64,
) -> io::Result<Option<String>> {
  if pointer == 0 {
    return Ok(None);
  }
  let mut bytes = Vec::new();
  for offset in 0..MAX_STRING_LENGTH {
    let address = pointer
      .checked_add(offset as u64)
      .ok_or_else(|| invalid_data("META string address overflows"))?;
    let byte = resource.read_address(address, 1)?[0];
    if byte == 0 {
      return Ok(Some(String::from_utf8_lossy(&bytes).into_owned()));
    }
    bytes.push(byte);
  }
  Err(invalid_data("META string exceeds maximum length"))
}

fn read_count(
  bytes: &[u8],
  offset: usize,
) -> io::Result<usize> {
  let count = read_i16(bytes, offset)?;
  if count < 0 {
    return Err(invalid_data("META table has a negative record count"));
  }
  Ok(count as usize)
}

fn read_u32(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u32> {
  Ok(u32::from_le_bytes(read_array(bytes, offset)?))
}

fn read_i32(
  bytes: &[u8],
  offset: usize,
) -> io::Result<i32> {
  Ok(i32::from_le_bytes(read_array(bytes, offset)?))
}

fn read_u64(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u64> {
  Ok(u64::from_le_bytes(read_array(bytes, offset)?))
}

fn read_i16(
  bytes: &[u8],
  offset: usize,
) -> io::Result<i16> {
  Ok(i16::from_le_bytes(read_array(bytes, offset)?))
}

fn read_array<const N: usize>(
  bytes: &[u8],
  offset: usize,
) -> io::Result<[u8; N]> {
  let end = offset.checked_add(N).ok_or_else(|| invalid_data("META record offset overflows"))?;
  bytes
    .get(offset..end)
    .ok_or_else(|| invalid_data("META record is truncated"))?
    .try_into()
    .map_err(|_| invalid_data("META record is truncated"))
}

fn invalid_data(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

fn jenk_hash(value: &str) -> u32 {
  let mut hash = 0u32;
  for byte in value.bytes() {
    hash = hash.wrapping_add(byte as u32);
    hash = hash.wrapping_add(hash << 10);
    hash ^= hash >> 6;
  }
  hash = hash.wrapping_add(hash << 3);
  hash ^= hash >> 11;
  hash.wrapping_add(hash << 15)
}

#[cfg(test)]
mod tests {
  use crate::core::format::gamefile::resource_file::Rsc7Resource;

  use super::MetaResource;

  #[test]
  fn parses_embedded_meta_tables_from_ymap() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
      .join("asset/extracted/brofx_mansion_06___apa_ch2_occl_05.ymap");
    let bytes = std::fs::read(fixture).unwrap();
    let resource = Rsc7Resource::decode(&bytes).unwrap();
    let meta = MetaResource::parse(&resource).unwrap();

    assert!(meta.root_block_index > 0);
    assert!(!meta.structures.is_empty());
    assert!(!meta.data_blocks.is_empty());
    let root = &meta.data_blocks[meta.root_block_index as usize - 1];
    assert!(meta.structures.iter().any(|s| s.name_hash == root.structure_name_hash));
  }

  #[test]
  fn parses_meta_tables_from_all_checked_in_ymaps() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/extracted");
    let mut parsed = 0;

    for entry in std::fs::read_dir(fixtures).unwrap() {
      let path = entry.unwrap().path();
      if path.extension().is_some_and(|extension| extension == "ymap") {
        let bytes = std::fs::read(&path).unwrap();
        let resource = Rsc7Resource::decode(&bytes).unwrap();
        MetaResource::parse(&resource).unwrap_or_else(|error| {
          panic!("failed to parse META in {}: {error}", path.display());
        });
        parsed += 1;
      }
    }

    assert!(parsed > 0);
  }
}
