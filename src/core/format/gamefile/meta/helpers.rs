use super::*;

pub(super) fn invalid_data(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Computes the Jenkins hash used by META name references.
///
/// A `const fn`, so field names known at compile time hash once.
pub(crate) const fn jenk_hash(value: &str) -> u32 {
  let bytes = value.as_bytes();
  let mut hash = 0u32;
  let mut index = 0;
  while index < bytes.len() {
    hash = hash.wrapping_add(bytes[index] as u32);
    hash = hash.wrapping_add(hash << 10);
    hash ^= hash >> 6;
    index += 1;
  }
  hash = hash.wrapping_add(hash << 3);
  hash ^= hash >> 11;
  hash.wrapping_add(hash << 15)
}
