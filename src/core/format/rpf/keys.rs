//! Archive keys: the AES key found in GTA5.exe and the NG material in CodeWalker's `magic.dat`.

use crate::platform::fs;
use std::{io, io::Read, path::Path};

use aes::{
  Aes256,
  cipher::{BlockCipherDecrypt, KeyInit},
};
use flate2::read::DeflateDecoder;
use rayon::prelude::*;
use sha1::{Digest, Sha1};

use super::invalid_data;

/// SHA-1 of the 32-byte PC AES key, used to locate it in GTA5.exe.
const AES_KEY_SHA1: [u8; 20] = [
  0xa0, 0x79, 0x61, 0x28, 0xa7, 0x75, 0x72, 0x0a, 0xc2, 0x04, 0xd9, 0x81, 0x9f, 0x68, 0xc1, 0x72,
  0xe3, 0x95, 0x2c, 0x6d,
];
/// Alignment of the executable offsets searched for the AES key.
const AES_KEY_ALIGNMENT: usize = 8;
const NG_KEY_COUNT: usize = 101;
const NG_KEY_WORDS: usize = 68;
const NG_ROUNDS: usize = 17;
const NG_KEYS_SIZE: usize = NG_KEY_COUNT * NG_KEY_WORDS * 4;
const NG_TABLES_SIZE: usize = NG_ROUNDS * 16 * 256 * 4;
const LUT_SIZE: usize = 256;

/// One NG round's 16 byte-indexed lookup tables.
pub(super) type NgRoundTables = [[u32; 256]; 16];

/// Decryption keys for PC GTA V Legacy RPF7 archives.
pub struct RpfKeys {
  pub(super) aes: Aes256,
  pub(super) ng_keys: Vec<[u32; NG_KEY_WORDS]>,
  pub(super) ng_tables: Vec<NgRoundTables>,
  pub(super) lut: [u8; LUT_SIZE],
}

impl RpfKeys {
  /// Locates the AES key in `executable` and decodes the NG keys, tables and name
  /// lookup table from CodeWalker's `magic.dat` with it.
  pub fn load(
    executable: &Path,
    magic: &Path,
  ) -> io::Result<Self> {
    let magic = fs::read(magic).map_err(|error| {
      io::Error::new(error.kind(), format!("Cannot read {} ({error})", magic.display()))
    })?;
    let executable = fs::read(executable).map_err(|error| {
      io::Error::new(error.kind(), format!("Cannot read {} ({error})", executable.display()))
    })?;
    Self::from_parts(&executable, &magic)
  }

  /// Builds the keys from executable and `magic.dat` bytes.
  pub fn from_parts(
    executable: &[u8],
    magic: &[u8],
  ) -> io::Result<Self> {
    let key = find_aes_key(executable)
      .ok_or_else(|| invalid_data("The archive AES key was not found in GTA5.exe"))?;
    let aes = Aes256::new_from_slice(&key).expect("AES-256 keys are 32 bytes");
    let data = decode_magic(magic, &key, &aes)?;
    let (keys, rest) = data.split_at(NG_KEYS_SIZE);
    let (tables, lut) = rest.split_at(NG_TABLES_SIZE);
    let mut words = keys.as_chunks::<4>().0.iter().map(|word| u32::from_le_bytes(*word));
    let ng_keys =
      (0..NG_KEY_COUNT).map(|_| std::array::from_fn(|_| words.next().unwrap())).collect();
    let mut words = tables.as_chunks::<4>().0.iter().map(|word| u32::from_le_bytes(*word));
    let ng_tables = (0..NG_ROUNDS)
      .map(|_| std::array::from_fn(|_| std::array::from_fn(|_| words.next().unwrap())))
      .collect();
    Ok(Self {
      aes,
      ng_keys,
      ng_tables,
      lut: lut[..LUT_SIZE].try_into().unwrap(),
    })
  }

  /// Keys for archives whose tables of contents are unencrypted.
  #[cfg(test)]
  pub(crate) fn unencrypted() -> Self {
    Self {
      aes: Aes256::new_from_slice(&[0; 32]).unwrap(),
      ng_keys: vec![[0; NG_KEY_WORDS]; NG_KEY_COUNT],
      ng_tables: vec![[[0; 256]; 16]; NG_ROUNDS],
      lut: [0; LUT_SIZE],
    }
  }
}

fn find_aes_key(executable: &[u8]) -> Option<[u8; 32]> {
  let last = executable.len().checked_sub(32)?;
  (0..=last / AES_KEY_ALIGNMENT)
    .into_par_iter()
    .map(|index| index * AES_KEY_ALIGNMENT)
    .find_first(|&offset| Sha1::digest(&executable[offset..offset + 32])[..] == AES_KEY_SHA1)
    .map(|offset| executable[offset..offset + 32].try_into().unwrap())
}

/// Reverses CodeWalker's encoding: masking with four .NET `Random` byte streams seeded
/// from the key hash, AES-ECB over whole blocks, then raw deflate.
fn decode_magic(
  magic: &[u8],
  key: &[u8; 32],
  aes: &Aes256,
) -> io::Result<Vec<u8>> {
  let mut data = magic.to_vec();
  let mut random = DotNetRandom::new(jenkins_hash(key) as i32);
  for _ in 0..4 {
    for byte in &mut data {
      *byte = byte.wrapping_sub(random.next_byte());
    }
  }
  decrypt_aes(aes, &mut data);
  let mut decoded = Vec::new();
  DeflateDecoder::new(&data[..])
    .read_to_end(&mut decoded)
    .map_err(|_| invalid_data("magic.dat does not decode with this GTA5.exe key"))?;
  if decoded.len() < NG_KEYS_SIZE + NG_TABLES_SIZE + LUT_SIZE {
    return Err(invalid_data("magic.dat is truncated"));
  }
  Ok(decoded)
}

/// Decrypts whole 16-byte blocks in ECB mode, leaving a trailing partial block as is.
pub(super) fn decrypt_aes(
  aes: &Aes256,
  data: &mut [u8],
) {
  for block in data.as_chunks_mut::<16>().0 {
    aes.decrypt_block(block.into());
  }
}

/// Bob Jenkins' one-at-a-time hash over raw bytes.
fn jenkins_hash(data: &[u8]) -> u32 {
  let mut hash = 0u32;
  for &byte in data {
    hash = hash.wrapping_add(u32::from(byte));
    hash = hash.wrapping_add(hash << 10);
    hash ^= hash >> 6;
  }
  hash = hash.wrapping_add(hash << 3);
  hash ^= hash >> 11;
  hash.wrapping_add(hash << 15)
}

/// The seeded `System.Random` generator (Knuth's subtractive method) of .NET.
struct DotNetRandom {
  seeds: [i32; 56],
  next: usize,
  next_p: usize,
}

impl DotNetRandom {
  fn new(seed: i32) -> Self {
    let mut seeds = [0; 56];
    let mut mj = 161_803_398 - if seed == i32::MIN { i32::MAX } else { seed.abs() };
    seeds[55] = mj;
    let mut mk = 1;
    let mut ii = 0;
    for _ in 1..55 {
      ii = (ii + 21) % 55;
      seeds[ii] = mk;
      mk = mj - mk;
      if mk < 0 {
        mk += i32::MAX;
      }
      mj = seeds[ii];
    }
    for _ in 1..5 {
      for i in 1..56 {
        seeds[i] = seeds[i].wrapping_sub(seeds[1 + (i + 30) % 55]);
        if seeds[i] < 0 {
          seeds[i] += i32::MAX;
        }
      }
    }
    Self {
      seeds,
      next: 0,
      next_p: 21,
    }
  }

  fn sample(&mut self) -> i32 {
    self.next = if self.next >= 55 { 1 } else { self.next + 1 };
    self.next_p = if self.next_p >= 55 { 1 } else { self.next_p + 1 };
    let mut value = self.seeds[self.next].wrapping_sub(self.seeds[self.next_p]);
    if value == i32::MAX {
      value -= 1;
    }
    if value < 0 {
      value += i32::MAX;
    }
    self.seeds[self.next] = value;
    value
  }

  fn next_byte(&mut self) -> u8 {
    self.sample() as u8
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn dotnet_random_matches_the_seeded_framework_sequence() {
    let mut random = DotNetRandom::new(0);
    let values: Vec<_> = (0..3).map(|_| random.sample()).collect();
    // `new System.Random(0).Next()` three times, and once for `int.MinValue`.
    assert_eq!(values, [1_559_595_546, 1_755_192_844, 1_649_316_166]);
    assert_eq!(DotNetRandom::new(i32::MIN).sample(), 1_559_595_546);
  }

  #[test]
  fn jenkins_hash_matches_one_at_a_time() {
    assert_eq!(jenkins_hash(b""), 0);
    assert_eq!(jenkins_hash(b"a"), 0xca2e9442);
  }

  #[test]
  fn aes_key_search_rejects_executables_without_the_key() {
    assert!(find_aes_key(&[0; 256]).is_none());
    assert!(find_aes_key(&[0; 31]).is_none());
  }
}
