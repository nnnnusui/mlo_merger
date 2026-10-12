use super::Rsc7Resource;
use crate::core::format::gamefile::test_support::{sample_ymap_binary, sample_ymap_xml};

#[test]
fn contiguous_resources_use_one_page_without_splitting_blocks() {
  for length in [1, 9_000, 50_000, 3_000_000] {
    let data = vec![0x5a; length];
    let resource = Rsc7Resource::from_pages(2, &data, &[]).unwrap();
    let flags = resource.system_flags;
    let count = ((flags >> 27) & 1)
      + ((flags >> 26) & 1)
      + ((flags >> 25) & 1)
      + ((flags >> 24) & 1)
      + ((flags >> 17) & 0x7f)
      + ((flags >> 11) & 0x3f)
      + ((flags >> 7) & 0xf)
      + ((flags >> 5) & 3)
      + ((flags >> 4) & 1);
    assert_eq!(count, 1);
    assert_eq!(&resource.system_data[..length], data.as_slice());
    assert_eq!(Rsc7Resource::decode(&resource.encode().unwrap()).unwrap(), resource);
  }
}

#[test]
fn decodes_sample_ymap_resource() {
  let xml = sample_ymap_xml("parent_refs/child.ymap.xml");
  let bytes = sample_ymap_binary(&xml);
  let resource = Rsc7Resource::decode(&bytes).unwrap();

  assert_eq!(resource.version, 2);
  assert!(!resource.system_data.is_empty());
  assert!(resource.graphics_data.is_empty());
}

#[test]
fn decodes_all_checked_in_ymap_resources() {
  let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/extracted");
  let mut decoded = 0;

  for entry in std::fs::read_dir(fixtures).unwrap() {
    let path = entry.unwrap().path();
    if path.extension().is_some_and(|extension| extension == "ymap") {
      let bytes = std::fs::read(&path).unwrap();
      Rsc7Resource::decode(&bytes).unwrap_or_else(|error| {
        panic!("failed to decode {}: {error}", path.display());
      });
      decoded += 1;
    }
  }

  assert!(decoded > 0);
}

#[test]
fn encode_decode_preserves_all_checked_in_ymap_pages() {
  let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/extracted");
  let mut round_tripped = 0;

  for entry in std::fs::read_dir(fixtures).unwrap() {
    let path = entry.unwrap().path();
    if path.extension().is_some_and(|extension| extension == "ymap") {
      let bytes = std::fs::read(&path).unwrap();
      let original = Rsc7Resource::decode(&bytes).unwrap();
      let encoded = original.encode().unwrap();
      let decoded = Rsc7Resource::decode(&encoded).unwrap();
      assert_eq!(decoded, original, "RSC7 page mismatch for {}", path.display());
      round_tripped += 1;
    }
  }

  assert!(round_tripped > 0);
}

#[test]
fn creates_page_flags_for_arbitrary_resource_data() {
  let system = vec![0x5a; 0x2345];
  let graphics = vec![0xa5; 0x401];
  let resource = Rsc7Resource::from_pages(2, &system, &graphics).unwrap();
  let encoded = resource.encode().unwrap();
  let decoded = Rsc7Resource::decode(&encoded).unwrap();

  assert_eq!(decoded.version, 2);
  assert_eq!(decoded.system_data.len(), 0x4000);
  assert_eq!(decoded.graphics_data.len(), 0x2000);
  assert_eq!(&decoded.system_data[..system.len()], system);
  assert_eq!(&decoded.graphics_data[..graphics.len()], graphics);
  assert!(decoded.system_data[system.len()..].iter().all(|byte| *byte == 0));
  assert!(decoded.graphics_data[graphics.len()..].iter().all(|byte| *byte == 0));
}

#[test]
fn rejects_non_rsc7_input() {
  let error = Rsc7Resource::decode(&[0; 16]).unwrap_err();
  assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn reads_system_and_graphics_virtual_addresses() {
  let resource = Rsc7Resource {
    version: 2,
    system_flags: 0,
    graphics_flags: 0,
    system_data: vec![1, 2, 3],
    graphics_data: vec![4, 5, 6],
  };

  assert_eq!(resource.read_address(0x5000_0001, 2).unwrap(), &[2, 3]);
  assert_eq!(resource.read_address(0x6000_0001, 2).unwrap(), &[5, 6]);
  assert!(resource.read_address(0x5000_0002, 2).is_err());
}
