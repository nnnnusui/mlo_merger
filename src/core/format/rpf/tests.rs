use std::io::{Read, Write};

use super::*;

/// Builds an OPEN (unencrypted table of contents) archive whose root holds `entries`.
fn fixture(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
  let mut names = vec![0];
  let mut offsets = vec![];
  for (name, _, _) in entries {
    offsets.push(names.len() as u64);
    names.extend_from_slice(name.as_bytes());
    names.push(0);
  }
  let mut bytes = vec![];
  for value in [RPF7_MAGIC, entries.len() as u32 + 1, names.len() as u32, ENCRYPTION_OPEN] {
    bytes.extend_from_slice(&value.to_le_bytes());
  }
  for value in [0u32, DIRECTORY_MARKER, 1, entries.len() as u32] {
    bytes.extend_from_slice(&value.to_le_bytes());
  }
  let mut block = (16 + (entries.len() + 1) * 16 + names.len()).div_ceil(512);
  let mut blocks = vec![];
  for ((_, data, resource), name_offset) in entries.iter().zip(offsets) {
    blocks.push(block);
    let size = if *resource { data.len() as u64 } else { 0 };
    let file_offset = block as u64 | if *resource { 0x800000 } else { 0 };
    bytes.extend_from_slice(&(name_offset | (size << 16) | (file_offset << 40)).to_le_bytes());
    bytes.extend_from_slice(
      &(if *resource { 0x08000000u32 } else { data.len() as u32 }).to_le_bytes(),
    );
    bytes.extend_from_slice(&0u32.to_le_bytes());
    block += data.len().div_ceil(512);
  }
  bytes.extend_from_slice(&names);
  for ((_, data, _), block) in entries.iter().zip(blocks) {
    bytes.resize(block * 512, 0);
    bytes.extend_from_slice(data);
  }
  bytes.resize(bytes.len().div_ceil(512) * 512, 0);
  bytes
}

#[test]
fn reads_nested_archives_and_rebuilds_resource_headers() {
  let payload = vec![42u8; 512];
  let mut encoder = DeflateEncoder::new(vec![], Compression::default());
  encoder.write_all(&payload).unwrap();
  let mut resource = b"RSC7".to_vec();
  for value in [0u32, 0x08000000, 0] {
    resource.extend_from_slice(&value.to_le_bytes());
  }
  resource.extend(encoder.finish().unwrap());
  let nested = fixture(&[
    ("Inside.ymap", &resource, true),
    ("collision.ybn", &resource, true),
    ("ignored.txt", b"skip", false),
  ]);
  let archive = fixture(&[("outside.ymap", b"outer", false), ("nested.rpf", &nested, false)]);
  let directory = std::env::temp_dir().join(format!("rpf_fixture_{}", std::process::id()));
  crate::fs::create_dir_all(&directory).unwrap();
  let input = directory.join("Fixture.rpf");
  crate::fs::write(&input, archive).unwrap();
  let keys = RpfKeys::unencrypted();

  let root = RpfArchive::open(&input, &keys).unwrap();
  let paths: Vec<_> = root.archives().iter().map(|archive| archive.path.as_str()).collect();
  assert_eq!(paths, ["fixture.rpf", "fixture.rpf/nested.rpf"]);
  let names: Vec<_> = root.files.iter().map(|file| file.path.as_str()).collect();
  assert_eq!(names, ["fixture.rpf/outside.ymap", "fixture.rpf/nested.rpf"]);
  let nested = &root.children[0];
  assert_eq!(nested.files[0].name, "Inside.ymap");
  assert_eq!(nested.files[0].path, "fixture.rpf/nested.rpf/inside.ymap");

  let mut file = File::open(&input).unwrap();
  assert_eq!(root.extract(&mut file, &root.files[0], &keys).unwrap(), b"outer");
  assert_eq!(nested.extract(&mut file, &nested.files[2], &keys).unwrap(), b"skip");
  for entry in &nested.files[..2] {
    let extracted = nested.extract(&mut file, entry, &keys).unwrap();
    assert_eq!(&extracted[..16], &resource[..16]);
    let mut decoded = vec![];
    DeflateDecoder::new(&extracted[16..]).read_to_end(&mut decoded).unwrap();
    assert_eq!(decoded, payload);
  }

  crate::fs::write(directory.join("invalid.rpf"), b"invalid archive").unwrap();
  assert!(RpfArchive::open(&directory.join("invalid.rpf"), &keys).is_err());
  crate::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn ng_rounds_mix_the_documented_byte_columns() {
  let mut tables = [[0u32; 256]; 16];
  for (index, table) in tables.iter_mut().enumerate() {
    table[1] = 1 << index;
  }
  let mut block = [1; 16];
  ng_round_b(&mut block, &[0; 4], &tables);
  let words: Vec<_> =
    block.as_chunks::<4>().0.iter().map(|word| u32::from_le_bytes(*word)).collect();
  let expected: Vec<_> =
    NG_ROUND_B.iter().map(|indices| indices.iter().map(|index| 1 << index).sum::<u32>()).collect();
  assert_eq!(words, expected);
  let mut block = [1; 16];
  ng_round_a(&mut block, &[1, 2, 3, 4], &tables);
  assert_eq!(&block[..4], &(0xf ^ 1u32).to_le_bytes());
}

#[test]
#[ignore = "requires an installed game in /mnt/gtav and magic.dat (MAGIC_DAT, default ./magic.dat)"]
fn gtav_keys_decrypt_installed_archives() {
  let magic = std::env::var("MAGIC_DAT").unwrap_or_else(|_| "magic.dat".into());
  let keys = RpfKeys::load(Path::new("/mnt/gtav/GTA5.exe"), Path::new(&magic)).unwrap();
  let input = Path::new("/mnt/gtav/update/update.rpf");
  let archive = RpfArchive::open(input, &keys).unwrap();
  let list = archive.files.iter().find(|file| file.path == "update.rpf/common/data/dlclist.xml");
  let mut file = File::open(input).unwrap();
  let list = archive.extract(&mut file, list.unwrap(), &keys).unwrap();
  assert!(String::from_utf8_lossy(&list).contains("dlcpacks:/"));
}
