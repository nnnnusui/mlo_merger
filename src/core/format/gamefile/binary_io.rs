//! Checked byte writes and aligned storage shared by native resource writers.

use std::io;

/// Writes one record field without permitting overflow or an out-of-bounds range.
pub(crate) fn write(
  bytes: &mut [u8],
  offset: usize,
  value: &[u8],
) -> io::Result<()> {
  let end =
    offset.checked_add(value.len()).ok_or_else(|| invalid("Binary write offset overflows"))?;
  bytes
    .get_mut(offset..end)
    .ok_or_else(|| invalid("Binary write range is out of bounds"))?
    .copy_from_slice(value);
  Ok(())
}

/// Pads native record storage to the next 16-byte boundary.
pub(crate) fn align_16(bytes: &mut Vec<u8>) -> io::Result<()> {
  let length =
    bytes.len().checked_add(15).ok_or_else(|| invalid("Binary output size overflows"))? & !15;
  bytes.resize(length, 0);
  Ok(())
}

/// Reserves zero-filled storage at a 16-byte boundary.
pub(crate) fn reserve(
  bytes: &mut Vec<u8>,
  length: usize,
) -> io::Result<usize> {
  align_16(bytes)?;
  let offset = bytes.len();
  let end = offset.checked_add(length).ok_or_else(|| invalid("Binary output size overflows"))?;
  bytes.resize(end, 0);
  Ok(offset)
}

/// Appends a native record at a 16-byte boundary.
pub(crate) fn append(
  bytes: &mut Vec<u8>,
  value: &[u8],
) -> io::Result<usize> {
  let offset = reserve(bytes, value.len())?;
  write(bytes, offset, value)?;
  Ok(offset)
}

fn invalid(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn native_writes_check_ranges_and_align_records() {
    let mut bytes = vec![7; 3];
    assert_eq!(reserve(&mut bytes, 4).unwrap(), 16);
    assert_eq!(bytes.len(), 20);
    assert_eq!(append(&mut bytes, &[1, 2]).unwrap(), 32);
    assert_eq!(&bytes[32..], &[1, 2]);
    assert!(write(&mut bytes, usize::MAX, &[1]).is_err());
    assert!(write(&mut bytes, 34, &[1]).is_err());
  }
}
