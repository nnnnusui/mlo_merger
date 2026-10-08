use super::*;

impl PsoResource {
  pub(super) fn address(
    &self,
    block: usize,
    offset: usize,
    length: usize,
  ) -> io::Result<usize> {
    let block = self
      .blocks
      .get(block.checked_sub(1).ok_or_else(|| invalid("null PSO block"))?)
      .ok_or_else(|| invalid("PSO pointer block is out of range"))?;
    let offset = if offset >= block.length { offset >> 8 } else { offset };
    if offset.checked_add(length).is_none_or(|end| end > block.length) {
      return Err(invalid("PSO field exceeds its block"));
    }
    Ok(block.offset + offset)
  }

  pub(super) fn pointer(
    &self,
    address: usize,
  ) -> io::Result<(usize, usize)> {
    slice(&self.data, address, 8)?;
    let pointer = u32_at(&self.data, address)? as u64;
    Ok(((pointer & 0xfff) as usize, ((pointer >> 12) & 0xfffff) as usize))
  }
}

pub(super) fn slice(
  bytes: &[u8],
  offset: usize,
  length: usize,
) -> io::Result<&[u8]> {
  let end = offset.checked_add(length).ok_or_else(|| invalid("PSO range overflow"))?;
  bytes.get(offset..end).ok_or_else(|| invalid("truncated PSO data"))
}

pub(super) fn u16_at(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u16> {
  Ok(u16::from_be_bytes(slice(bytes, offset, 2)?.try_into().unwrap()))
}

pub(super) fn u32_at(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u32> {
  Ok(u32::from_be_bytes(slice(bytes, offset, 4)?.try_into().unwrap()))
}

pub(super) fn invalid(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

pub(super) fn xml_hash(value: &str) -> io::Result<u32> {
  let value = value.trim();
  if value.is_empty() {
    return Ok(0);
  }
  if let Some(hex) = value.strip_prefix("hash_") {
    u32::from_str_radix(hex, 16).map_err(|_| invalid("invalid explicit PSO hash"))
  } else {
    Ok(jenk_hash(value))
  }
}

pub(super) fn float_text(value: &str) -> io::Result<f32> {
  value.parse().map_err(|_| invalid("invalid PSO float"))
}

pub(super) fn resolve(
  hash: u32,
  names: &HashMap<u32, String>,
) -> String {
  if hash == 0 {
    String::new()
  } else {
    names.get(&hash).cloned().unwrap_or_else(|| format!("hash_{hash:08X}"))
  }
}
