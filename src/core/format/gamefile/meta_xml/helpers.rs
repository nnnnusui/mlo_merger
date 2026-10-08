use super::*;

pub(super) fn read_string_pointer(
  meta: &MetaResource,
  bytes: &[u8],
) -> io::Result<String> {
  let pointer = u64_at(bytes, 0)?;
  let block_id = (pointer & 0xfff) as usize;
  if block_id == 0 {
    return Ok(String::new());
  }
  let offset = ((pointer >> 12) & 0xfffff) as usize;
  let count = u16_at(bytes, 8)? as usize;
  let block = meta
    .data_blocks
    .get(block_id - 1)
    .ok_or_else(|| invalid_data("META string pointer references a missing block"))?;
  Ok(String::from_utf8_lossy(slice(&block.data, offset, count)?).trim_end_matches('\0').to_string())
}

pub(super) fn array_of_chars(
  block: &crate::core::format::gamefile::meta_resource::MetaDataBlock,
  offset: usize,
  count: usize,
) -> io::Result<String> {
  let bytes = slice(&block.data, offset, count)?;
  Ok(String::from_utf8_lossy(bytes).trim_end_matches('\0').to_string())
}

pub(super) fn field_size(data_type: u8) -> usize {
  match data_type {
    0x01 | 0x10 | 0x11 => 1,
    0x12 | 0x13 | 0x60 | 0x64 => 2,
    0x14 | 0x15 | 0x21 | 0x4a | 0x62 | 0x63 | 0x65 => 4,
    0x07 | 0x34 | 0x44 | 0x52 => 16,
    0x33 => 12,
    0x05 | 0x40 | 0x50 => 0,
    0x59 => 8,
    _ => 0,
  }
}

pub(super) fn resolve_hash(
  hash: u32,
  names: &HashMap<u32, String>,
) -> String {
  if hash == 0 { String::new() } else { resolve_name(hash, names) }
}

pub(super) fn resolve_name(
  hash: u32,
  names: &HashMap<u32, String>,
) -> String {
  names.get(&hash).cloned().unwrap_or_else(|| format!("hash_{hash:08X}"))
}

pub(super) fn jenk_hash(value: &str) -> u32 {
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

pub(super) fn format_float(value: f32) -> String {
  if value == 0.0 && value.is_sign_positive() { "0".to_string() } else { value.to_string() }
}

pub(super) fn slice(
  bytes: &[u8],
  offset: usize,
  length: usize,
) -> io::Result<&[u8]> {
  let end = offset.checked_add(length).ok_or_else(|| invalid_data("META data range overflows"))?;
  bytes.get(offset..end).ok_or_else(|| invalid_data("META data range is out of bounds"))
}

pub(super) fn u16_at(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u16> {
  Ok(u16::from_le_bytes(slice(bytes, offset, 2)?.try_into().unwrap()))
}

pub(super) fn i16_at(
  bytes: &[u8],
  offset: usize,
) -> io::Result<i16> {
  Ok(i16::from_le_bytes(slice(bytes, offset, 2)?.try_into().unwrap()))
}

pub(super) fn u32_at(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u32> {
  Ok(u32::from_le_bytes(slice(bytes, offset, 4)?.try_into().unwrap()))
}

pub(super) fn i32_at(
  bytes: &[u8],
  offset: usize,
) -> io::Result<i32> {
  Ok(i32::from_le_bytes(slice(bytes, offset, 4)?.try_into().unwrap()))
}

pub(super) fn u64_at(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u64> {
  Ok(u64::from_le_bytes(slice(bytes, offset, 8)?.try_into().unwrap()))
}

pub(super) fn f32_at(
  bytes: &[u8],
  offset: usize,
) -> io::Result<f32> {
  Ok(f32::from_le_bytes(slice(bytes, offset, 4)?.try_into().unwrap()))
}

pub(super) fn invalid_data(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}
