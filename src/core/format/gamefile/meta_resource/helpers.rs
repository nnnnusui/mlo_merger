use super::*;

pub(super) fn invalid_data(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Computes the Jenkins hash used by META name references.
pub(crate) fn jenk_hash(value: &str) -> u32 {
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
