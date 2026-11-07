//! PC GTA V Legacy RPF7 archive reading, following CodeWalker's `RpfFile`.
//!
//! The table of contents is decrypted with [`RpfKeys`]; nested `.rpf` entries are scanned
//! recursively. Paths are lowercase, `/`-separated and start with the root archive's name.

use crate::platform::fs::File;
use std::{
  io::{self, Read, Seek, SeekFrom, Write},
  path::Path,
};

use flate2::{Compression, read::DeflateDecoder, write::DeflateEncoder};

mod keys;

pub use keys::RpfKeys;
use keys::{NgRoundTables, decrypt_aes};

const RPF7_MAGIC: u32 = 0x5250_4637;
const RSC7_MAGIC: u32 = 0x3743_5352;
const ENCRYPTION_NONE: u32 = 0;
const ENCRYPTION_OPEN: u32 = 0x4e45_504f;
const ENCRYPTION_AES: u32 = 0x0fff_fff9;
const DIRECTORY_MARKER: u32 = 0x7fff_ff00;
const BLOCK_SIZE: u64 = 512;
/// Resource sizes that do not fit in 24 bits are stored in the resource header.
const LARGE_RESOURCE_SIZE: u32 = 0xff_ffff;
const MAX_NAME_CHARS: usize = 256;

/// Byte indices feeding each output word of the NG mixing rounds.
const NG_ROUND_B: [[usize; 4]; 4] = [[0, 7, 10, 13], [1, 4, 11, 14], [2, 5, 8, 15], [3, 6, 9, 12]];

fn invalid_data(message: impl Into<String>) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// One RPF7 archive and its nested archives.
#[derive(Debug, Clone)]
pub struct RpfArchive {
  /// Lowercase virtual path of this archive.
  pub path: String,
  /// File entries in table-of-contents order, nested archives included.
  pub files: Vec<RpfFile>,
  /// Nested archives in table-of-contents order.
  pub children: Vec<RpfArchive>,
  start: u64,
  aes_encrypted: bool,
}

/// One file entry of an archive.
#[derive(Debug, Clone)]
pub struct RpfFile {
  /// Entry name as stored, possibly mixed case.
  pub name: String,
  /// Lowercase virtual path, starting with the root archive's name.
  pub path: String,
  kind: FileKind,
}

#[derive(Debug, Clone)]
enum FileKind {
  Binary {
    offset: u32,
    size: u32,
    uncompressed_size: u32,
    encrypted: bool,
  },
  Resource {
    offset: u32,
    size: u32,
    system_flags: u32,
    graphics_flags: u32,
  },
}

enum Entry {
  Directory {
    name_offset: u32,
    first: u32,
    count: u32,
  },
  File {
    name_offset: u32,
    kind: FileKind,
  },
}

impl RpfFile {
  /// Lowercase name, as CodeWalker reports it.
  pub fn name_lower(&self) -> String {
    self.name.to_lowercase()
  }
}

impl RpfArchive {
  /// Reads the table of contents of `path` and of every nested archive.
  pub fn open(
    path: &Path,
    keys: &RpfKeys,
  ) -> io::Result<Self> {
    let name = path
      .file_name()
      .and_then(|name| name.to_str())
      .ok_or_else(|| invalid_data(format!("Invalid archive name: {}", path.display())))?;
    let mut file = File::open(path)?;
    let size = file.metadata()?.len();
    Self::read(&mut file, keys, 0, name, &name.to_lowercase(), size)
      .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))
  }

  fn read(
    file: &mut File,
    keys: &RpfKeys,
    start: u64,
    name: &str,
    path: &str,
    size: u64,
  ) -> io::Result<Self> {
    file.seek(SeekFrom::Start(start))?;
    let mut header = [0; 16];
    file.read_exact(&mut header)?;
    let word =
      |index: usize| u32::from_le_bytes(header[index * 4..index * 4 + 4].try_into().unwrap());
    if word(0) != RPF7_MAGIC {
      return Err(invalid_data(format!("{path} is not an RPF7 archive")));
    }
    let (entry_count, names_length, encryption) = (word(1), word(2), word(3));
    let mut entries = vec![0; entry_count as usize * 16];
    let mut names = vec![0; names_length as usize];
    file.read_exact(&mut entries)?;
    file.read_exact(&mut names)?;
    let aes_encrypted = encryption == ENCRYPTION_AES;
    match encryption {
      ENCRYPTION_NONE | ENCRYPTION_OPEN => {}
      ENCRYPTION_AES => {
        decrypt_aes(&keys.aes, &mut entries);
        decrypt_aes(&keys.aes, &mut names);
      }
      // CodeWalker treats any other value as NG.
      _ => {
        keys.decrypt_ng(&mut entries, name, size as u32)?;
        keys.decrypt_ng(&mut names, name, size as u32)?;
      }
    }

    let mut parsed = Vec::with_capacity(entry_count as usize);
    for entry in entries.as_chunks::<16>().0 {
      parsed.push(parse_entry(file, start, entry)?);
    }
    let read_name = |offset: u32| -> io::Result<String> {
      let bytes = names
        .get(offset as usize..)
        .ok_or_else(|| invalid_data(format!("{path}: entry name is outside the name table")))?;
      let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(bytes.len());
      Ok(String::from_utf8_lossy(&bytes[..end]).chars().take(MAX_NAME_CHARS).collect())
    };

    let mut paths: Vec<Option<String>> = vec![None; parsed.len()];
    if !matches!(parsed.first(), Some(Entry::Directory { .. })) {
      return Err(invalid_data(format!("{path}: the first entry is not a directory")));
    }
    paths[0] = Some(path.to_owned());
    let mut stack = vec![0];
    while let Some(index) = stack.pop() {
      let Entry::Directory {
        first,
        count,
        ..
      } = parsed[index]
      else {
        unreachable!()
      };
      let parent = paths[index].clone().unwrap();
      let children = first as usize..first as usize + count as usize;
      let entries = parsed
        .get(children.clone())
        .ok_or_else(|| invalid_data(format!("{path}: directory entry is out of range")))?;
      for (child, entry) in children.zip(entries) {
        let name_offset = match entry {
          Entry::Directory {
            name_offset,
            ..
          }
          | Entry::File {
            name_offset,
            ..
          } => *name_offset,
        };
        paths[child] = Some(format!("{parent}/{}", read_name(name_offset)?.to_lowercase()));
        if matches!(entry, Entry::Directory { .. }) {
          stack.push(child);
        }
      }
    }

    let mut files = Vec::new();
    for (entry, entry_path) in parsed.into_iter().zip(paths) {
      if let Entry::File {
        name_offset,
        kind,
      } = entry
      {
        let path = entry_path
          .ok_or_else(|| invalid_data(format!("{path}: file entry is outside every directory")))?;
        files.push(RpfFile {
          name: read_name(name_offset)?,
          path,
          kind,
        });
      }
    }
    let mut children = Vec::new();
    for entry in &files {
      if let FileKind::Binary {
        offset,
        size,
        uncompressed_size,
        ..
      } = entry.kind
        && entry.path.ends_with(".rpf")
        && is_valid_path(&entry.path)
      {
        let length = if size == 0 { uncompressed_size } else { size };
        children.push(Self::read(
          file,
          keys,
          start + u64::from(offset) * BLOCK_SIZE,
          &entry.name,
          &entry.path,
          u64::from(length),
        )?);
      }
    }
    Ok(Self {
      path: path.to_owned(),
      files,
      children,
      start,
      aes_encrypted,
    })
  }

  /// Visits this archive and its nested archives depth-first, parents first.
  pub fn archives(&self) -> Vec<&RpfArchive> {
    let mut archives = vec![self];
    for child in &self.children {
      archives.extend(child.archives());
    }
    archives
  }

  /// Reads `entry` of this archive as a standalone file.
  ///
  /// Binary files are decrypted and inflated. Resources are inflated and recompressed
  /// behind an RSC7 header built from their table-of-contents flags.
  pub fn extract(
    &self,
    file: &mut File,
    entry: &RpfFile,
    keys: &RpfKeys,
  ) -> io::Result<Vec<u8>> {
    match entry.kind {
      FileKind::Binary {
        offset,
        size,
        uncompressed_size,
        encrypted,
      } => {
        let length = if size == 0 { uncompressed_size } else { size };
        let mut data = self.read_bytes(file, offset, 0, length)?;
        if encrypted {
          if self.aes_encrypted {
            decrypt_aes(&keys.aes, &mut data);
          } else {
            keys.decrypt_ng(&mut data, &entry.name, uncompressed_size)?;
          }
        }
        if size == 0 {
          Ok(data)
        } else {
          inflate(&data).map_err(|_| invalid_data(format!("Could not decompress {}", entry.path)))
        }
      }
      FileKind::Resource {
        offset,
        size,
        system_flags,
        graphics_flags,
      } => {
        if size < 16 {
          return Err(invalid_data(format!("{} has no resource data", entry.path)));
        }
        let mut data = self.read_bytes(file, offset, 16, size - 16)?;
        // CodeWalker cannot tell which resources are encrypted beyond scripts.
        if entry.path.ends_with(".ysc") {
          if self.aes_encrypted {
            decrypt_aes(&keys.aes, &mut data);
          } else {
            keys.decrypt_ng(&mut data, &entry.name, size)?;
          }
        }
        // Resources that fail to inflate are kept as stored, as CodeWalker does.
        let data = inflate(&data).unwrap_or(data);
        let version = (system_flags >> 28) << 4 | graphics_flags >> 28;
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&data)?;
        let compressed = encoder.finish()?;
        let mut bytes = Vec::with_capacity(16 + compressed.len());
        for value in [RSC7_MAGIC, version, system_flags, graphics_flags] {
          bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&compressed);
        Ok(bytes)
      }
    }
  }

  fn read_bytes(
    &self,
    file: &mut File,
    offset: u32,
    skip: u64,
    length: u32,
  ) -> io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(self.start + u64::from(offset) * BLOCK_SIZE + skip))?;
    let mut data = vec![0; length as usize];
    file.read_exact(&mut data)?;
    Ok(data)
  }
}

fn parse_entry(
  file: &mut File,
  start: u64,
  entry: &[u8],
) -> io::Result<Entry> {
  let word = |index: usize| u32::from_le_bytes(entry[index * 4..index * 4 + 4].try_into().unwrap());
  let three =
    |index: usize| u32::from_le_bytes([entry[index], entry[index + 1], entry[index + 2], 0]);
  if word(1) == DIRECTORY_MARKER {
    return Ok(Entry::Directory {
      name_offset: word(0),
      first: word(2),
      count: word(3),
    });
  }
  if word(1) & 0x8000_0000 == 0 {
    let packed = u64::from_le_bytes(entry[..8].try_into().unwrap());
    let encrypted = match word(3) {
      0 => false,
      1 => true,
      _ => return Err(invalid_data("Unknown RPF7 file encryption type")),
    };
    return Ok(Entry::File {
      name_offset: (packed & 0xffff) as u32,
      kind: FileKind::Binary {
        offset: (packed >> 40) as u32 & 0xff_ffff,
        size: (packed >> 16) as u32 & 0xff_ffff,
        uncompressed_size: word(2),
        encrypted,
      },
    });
  }
  let offset = three(5) & 0x7f_ffff;
  let mut size = three(2);
  if size == LARGE_RESOURCE_SIZE {
    let mut header = [0; 16];
    file.seek(SeekFrom::Start(start + u64::from(offset) * BLOCK_SIZE))?;
    file.read_exact(&mut header)?;
    size = u32::from_le_bytes([header[7], header[14], header[5], header[2]]);
  }
  Ok(Entry::File {
    name_offset: u32::from(u16::from_le_bytes([entry[0], entry[1]])),
    kind: FileKind::Resource {
      offset,
      size,
      system_flags: word(2),
      graphics_flags: word(3),
    },
  })
}

fn inflate(data: &[u8]) -> io::Result<Vec<u8>> {
  let mut inflated = Vec::new();
  DeflateDecoder::new(data).read_to_end(&mut inflated)?;
  Ok(inflated)
}

/// CodeWalker skips nested archives whose paths look hostile.
fn is_valid_path(path: &str) -> bool {
  !path.is_empty()
    && path.chars().count() <= 500
    && !path.contains([':', ';'])
    && path.matches(['/', '\\']).count() <= 20
}

impl RpfKeys {
  fn ng_hash(
    &self,
    name: &str,
  ) -> io::Result<u32> {
    let mut hash = 0u32;
    for unit in name.encode_utf16() {
      let value = *self
        .lut
        .get(usize::from(unit))
        .ok_or_else(|| invalid_data(format!("Unsupported character in archive name: {name}")))?;
      let temp = 1025u32.wrapping_mul(u32::from(value).wrapping_add(hash));
      hash = (temp >> 6) ^ temp;
    }
    let hash = hash.wrapping_mul(9);
    Ok(32769u32.wrapping_mul((hash >> 11) ^ hash))
  }

  /// Decrypts whole 16-byte blocks with the NG key selected by `name` and `length`.
  fn decrypt_ng(
    &self,
    data: &mut [u8],
    name: &str,
    length: u32,
  ) -> io::Result<()> {
    let index = self.ng_hash(name)?.wrapping_add(length).wrapping_add(61) % 101;
    let key = &self.ng_keys[index as usize];
    for block in data.as_chunks_mut::<16>().0 {
      let round_key = |round: usize| -> &[u32] { &key[round * 4..round * 4 + 4] };
      ng_round_a(block, round_key(0), &self.ng_tables[0]);
      ng_round_a(block, round_key(1), &self.ng_tables[1]);
      for round in 2..16 {
        ng_round_b(block, round_key(round), &self.ng_tables[round]);
      }
      ng_round_a(block, round_key(16), &self.ng_tables[16]);
    }
    Ok(())
  }
}

fn ng_round_a(
  block: &mut [u8; 16],
  key: &[u32],
  tables: &NgRoundTables,
) {
  ng_round(block, key, tables, [[0, 1, 2, 3], [4, 5, 6, 7], [8, 9, 10, 11], [12, 13, 14, 15]]);
}

fn ng_round_b(
  block: &mut [u8; 16],
  key: &[u32],
  tables: &NgRoundTables,
) {
  ng_round(block, key, tables, NG_ROUND_B);
}

fn ng_round(
  block: &mut [u8; 16],
  key: &[u32],
  tables: &NgRoundTables,
  inputs: [[usize; 4]; 4],
) {
  let mut output = [0; 16];
  for (word, indices) in inputs.iter().enumerate() {
    let value = indices
      .iter()
      .fold(key[word], |value, &index| value ^ tables[index][usize::from(block[index])]);
    output[word * 4..word * 4 + 4].copy_from_slice(&value.to_le_bytes());
  }
  *block = output;
}

#[cfg(test)]
mod tests;
