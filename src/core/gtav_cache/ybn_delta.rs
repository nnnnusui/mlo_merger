//! Exact YBN byte deltas, preserving resource headers and unmodeled collision data.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{CachedFile, Result};

#[derive(Deserialize, Serialize)]
enum DeltaFormat {
  #[serde(rename = "vanilla_ybn_delta_v1")]
  VanillaYbnV1,
}

/// Replaces a changed byte span while retaining an identical prefix and suffix.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VanillaYbnDelta {
  format: DeltaFormat,
  /// Previous original YBN or delta in the version chain.
  pub(crate) base: CachedFile,
  /// SHA-256 of the exact target standalone binary.
  pub(crate) target_native_sha256: String,
  before_size: usize,
  after_size: usize,
  prefix_length: usize,
  suffix_length: usize,
  replacement: Vec<u8>,
}

fn hash(bytes: &[u8]) -> String {
  format!("{:x}", Sha256::digest(bytes))
}

impl VanillaYbnDelta {
  /// Captures the entire binary difference without requiring a YBN schema adapter.
  pub(crate) fn extract_from(
    before: &[u8],
    after: &[u8],
    base: CachedFile,
  ) -> Result<Self> {
    if hash(before) != base.sha256 {
      return Err("YBN delta predecessor binary hash mismatch".into());
    }
    let prefix_length =
      before.iter().zip(after).take_while(|(before, after)| before == after).count();
    let suffix_length = before[prefix_length..]
      .iter()
      .rev()
      .zip(after[prefix_length..].iter().rev())
      .take_while(|(before, after)| before == after)
      .count();
    Ok(Self {
      format: DeltaFormat::VanillaYbnV1,
      base,
      target_native_sha256: hash(after),
      before_size: before.len(),
      after_size: after.len(),
      prefix_length,
      suffix_length,
      replacement: after[prefix_length..after.len() - suffix_length].to_vec(),
    })
  }

  /// Reconstructs the byte-identical target and rejects invalid predecessors or spans.
  pub(crate) fn apply_to(
    &self,
    before: &[u8],
  ) -> Result<Vec<u8>> {
    if before.len() != self.before_size || hash(before) != self.base.sha256 {
      return Err("YBN delta predecessor binary hash mismatch".into());
    }
    let preserved =
      self.prefix_length.checked_add(self.suffix_length).ok_or("YBN delta span overflow")?;
    if preserved > before.len()
      || preserved.checked_add(self.replacement.len()) != Some(self.after_size)
    {
      return Err("Invalid YBN delta byte span".into());
    }
    let mut result = Vec::new();
    result.try_reserve_exact(self.after_size)?;
    result.extend_from_slice(&before[..self.prefix_length]);
    result.extend_from_slice(&self.replacement);
    result.extend_from_slice(&before[before.len() - self.suffix_length..]);
    if hash(&result) != self.target_native_sha256 {
      return Err("YBN delta reconstructed binary hash mismatch".into());
    }
    Ok(result)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn base(bytes: &[u8]) -> CachedFile {
    CachedFile {
      sha256: hash(bytes),
      object: "0000-base/ybn/collision.ybn".into(),
      native: None,
      source: "fixture.rpf/collision.ybn".into(),
    }
  }

  #[test]
  fn ybn_delta_round_trips_replacements_insertions_deletions_and_empty_files() {
    for (before, after) in [
      (&b"RSC7prefix-old-suffix"[..], &b"RSC7prefix-new-data-suffix"[..]),
      (&b"same"[..], &b"same"[..]),
      (&b""[..], &b"new"[..]),
      (&b"old"[..], &b""[..]),
      (&b"abc"[..], &b"abcd"[..]),
      (&b"abcd"[..], &b"acd"[..]),
    ] {
      let delta = VanillaYbnDelta::extract_from(before, after, base(before)).unwrap();
      let json = serde_json::to_vec(&delta).unwrap();
      let loaded: VanillaYbnDelta = serde_json::from_slice(&json).unwrap();
      assert_eq!(loaded.apply_to(before).unwrap(), after);
    }
  }

  #[test]
  fn ybn_delta_rejects_corrupt_predecessors_payloads_and_spans() {
    let before = b"RSC7old-data";
    let after = b"RSC7new-data";
    let delta = VanillaYbnDelta::extract_from(before, after, base(before)).unwrap();
    assert!(delta.apply_to(b"wrong").is_err());
    let mut delta = VanillaYbnDelta::extract_from(before, after, base(before)).unwrap();
    delta.replacement[0] ^= 1;
    assert!(delta.apply_to(before).is_err());
    let mut delta = VanillaYbnDelta::extract_from(before, after, base(before)).unwrap();
    delta.prefix_length = usize::MAX;
    assert!(delta.apply_to(before).is_err());
  }
}
