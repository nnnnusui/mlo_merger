//! Name references as they are stored in YMAPs: a name or an explicit `hash_XXXXXXXX`.

use crate::core::format::gamefile::meta::jenk_hash;

/// Resolves a named or explicitly hashed reference, including null references.
pub(crate) fn reference_hash(value: &str) -> u32 {
  let value = value.trim();
  if value.is_empty() {
    return 0;
  }
  if let Some(hash) = value.strip_prefix("hash_")
    && let Ok(hash) = u32::from_str_radix(hash, 16)
  {
    return hash;
  }
  jenk_hash(&value.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn names_and_hashes_are_the_same_reference() {
    assert_eq!(
      reference_hash("Parent_Map"),
      reference_hash(&format!("hash_{:08X}", jenk_hash("parent_map")))
    );
    assert_eq!(reference_hash(""), reference_hash("hash_00000000"));
  }
}
