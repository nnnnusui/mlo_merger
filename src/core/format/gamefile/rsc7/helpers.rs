use std::io;

pub(super) fn page_data_size(flags: u32) -> io::Result<usize> {
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

pub(super) fn flags_for_data(
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

pub(super) fn read_u32(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u32> {
  let end = offset.checked_add(4).ok_or_else(|| invalid_data("RSC7 header offset overflow"))?;
  let value = bytes.get(offset..end).ok_or_else(|| invalid_data("RSC7 header is truncated"))?;
  Ok(u32::from_le_bytes(value.try_into().expect("four-byte slice")))
}

pub(super) fn invalid_data(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}
