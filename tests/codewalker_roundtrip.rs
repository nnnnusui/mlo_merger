//! Round-trip smoke test for the CodeWalker.Bridge hosting layer.
//!
//! Ignored by default since it requires a locally built `bridge/CodeWalker.Bridge`
//! (see build.rs / CODEWALKER_CORE_DLL). Run with:
//!   CODEWALKER_CORE_DLL=/path/to/CodeWalker.Core.dll cargo test --test codewalker_roundtrip -- --ignored

use std::path::Path;

use mlo_merger::core::codewalker::CodeWalker;

#[test]
#[ignore]
fn ymap_xml_round_trip() {
  let bridge_dll = std::env::var("CODEWALKER_BRIDGE_DLL")
    .unwrap_or_else(|_| "bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll".to_string());
  let codewalker = CodeWalker::init(Path::new(&bridge_dll)).expect("failed to init CodeWalker");

  let sample = Path::new("asset/extracted/brofx_mansion_06___apa_ch2_06_strm_2.ymap");
  if !sample.exists() {
    eprintln!("sample fixture not present, skipping");
    return;
  }

  let tmp_dir = std::env::temp_dir().join("mlo_merger_codewalker_roundtrip_test");
  std::fs::create_dir_all(&tmp_dir).unwrap();
  let xml_out = tmp_dir.join("out.ymap.xml");
  let ymap_out = tmp_dir.join("out.ymap");

  codewalker.preload_names(sample).unwrap();
  codewalker.ymap_to_xml(sample, &xml_out).unwrap();
  assert!(xml_out.exists());

  codewalker.xml_to_ymap(&xml_out, &ymap_out).unwrap();
  assert!(ymap_out.exists());

  // re-exporting the rebuilt binary should reproduce identical xml
  let xml_out2 = tmp_dir.join("out2.ymap.xml");
  codewalker.ymap_to_xml(&ymap_out, &xml_out2).unwrap();
  let first = std::fs::read_to_string(&xml_out).unwrap();
  let second = std::fs::read_to_string(&xml_out2).unwrap();
  assert_eq!(first, second);
}
