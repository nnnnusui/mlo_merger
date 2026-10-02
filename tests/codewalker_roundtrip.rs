//! Round-trip smoke test for the CodeWalker.Bridge hosting layer.
//!
//! Ignored by default since it requires a locally built `bridge/CodeWalker.Bridge`
//! (see build.rs / CODEWALKER_CORE_DLL). Run with:
//!   CODEWALKER_CORE_DLL=/path/to/CodeWalker.Core.dll cargo test --test codewalker_roundtrip -- --ignored

use std::path::Path;

use mlo_merger::core::codewalker::CodeWalker;
use mlo_merger::core::format::gamefile::{
  meta_resource::MetaResource, meta_xml::ymap_to_xml, resource_file::Rsc7Resource,
};
use quick_xml::Reader;
use quick_xml::events::Event;

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

#[test]
#[ignore]
fn native_ymap_xml_matches_codewalker() {
  let bridge_dll = std::env::var("CODEWALKER_BRIDGE_DLL")
    .unwrap_or_else(|_| "bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll".to_string());
  let codewalker = CodeWalker::init(Path::new(&bridge_dll)).expect("failed to init CodeWalker");
  let sample = Path::new("asset/extracted/brofx_mansion_06___apa_ch2_occl_05.ymap");
  let tmp_dir = std::env::temp_dir().join("mlo_merger_native_codewalker_compare");
  std::fs::create_dir_all(&tmp_dir).unwrap();
  let codewalker_xml_path = tmp_dir.join("codewalker.ymap.xml");

  codewalker.preload_names(sample).unwrap();
  codewalker.ymap_to_xml(sample, &codewalker_xml_path).unwrap();
  let expected = std::fs::read_to_string(codewalker_xml_path).unwrap();
  let bytes = std::fs::read(sample).unwrap();
  let resource = Rsc7Resource::decode(&bytes).unwrap();
  let hash_names = MetaResource::parse(&resource).unwrap().hash_names();
  let native = ymap_to_xml(&bytes, &hash_names).unwrap();

  let native_events = canonical_xml(&native);
  let expected_events = canonical_xml(&expected);
  for (index, (native_event, expected_event)) in
    native_events.iter().zip(&expected_events).enumerate()
  {
    assert_eq!(
      native_event,
      expected_event,
      "XML mismatch at event {index}: native='{}', CodeWalker='{}'",
      truncate(native_event),
      truncate(expected_event)
    );
  }
  assert_eq!(native_events.len(), expected_events.len(), "XML event count differs");
}

#[test]
#[ignore]
fn native_ymap_xml_matches_codewalker_corpus() {
  let bridge_dll = std::env::var("CODEWALKER_BRIDGE_DLL")
    .unwrap_or_else(|_| "bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll".to_string());
  let codewalker = CodeWalker::init(Path::new(&bridge_dll)).expect("failed to init CodeWalker");
  let input_dir = Path::new("asset/extracted");
  let tmp_dir = std::env::temp_dir().join("mlo_merger_native_codewalker_corpus");
  std::fs::create_dir_all(&tmp_dir).unwrap();
  let inputs = std::fs::read_dir(input_dir)
    .unwrap()
    .map(|entry| entry.unwrap().path())
    .filter(|path| path.extension().is_some_and(|extension| extension == "ymap"))
    .filter(|path| {
      std::env::var("NATIVE_XML_COMPARE_FILTER")
        .ok()
        .is_none_or(|filter| path.file_name().unwrap().to_string_lossy().contains(&filter))
    })
    .collect::<Vec<_>>();

  for input in &inputs {
    codewalker.preload_names(input).unwrap();
  }

  let mut shared_names = std::collections::HashMap::new();
  let mut references = Vec::new();
  for input in &inputs {
    let bytes = std::fs::read(input).unwrap();
    let resource = Rsc7Resource::decode(&bytes).unwrap();
    shared_names.extend(MetaResource::parse(&resource).unwrap().hash_names());

    let output = tmp_dir.join(format!("{}.xml", input.file_name().unwrap().to_string_lossy()));
    codewalker.ymap_to_xml(input, &output).unwrap();
    references.push((input.clone(), output));
  }

  let mut compared = 0usize;
  let mut codewalker_errors = 0usize;
  for (input, reference_path) in &references {
    let bytes = std::fs::read(input).unwrap();
    let native = ymap_to_xml(&bytes, &shared_names)
      .unwrap_or_else(|error| panic!("native conversion failed for {}: {error}", input.display()));
    let expected = std::fs::read_to_string(reference_path).unwrap();
    if expected.contains("<error>") {
      codewalker_errors += 1;
      continue;
    }
    let native_events = canonical_xml(&native);
    let expected_events = canonical_xml(&expected);
    for (index, (native_event, expected_event)) in
      native_events.iter().zip(&expected_events).enumerate()
    {
      let native_context = native_events[index.saturating_sub(3)..index].join(" | ");
      let expected_context = expected_events[index.saturating_sub(3)..index].join(" | ");
      assert_eq!(
        native_event,
        expected_event,
        "{} differs from CodeWalker at XML event {index}: native='{}', CodeWalker='{}'; native context=[{}], CodeWalker context=[{}]",
        input.display(),
        truncate(native_event),
        truncate(expected_event),
        native_context,
        expected_context
      );
    }
    assert_eq!(
      native_events.len(),
      expected_events.len(),
      "{} has a different XML event count",
      input.display()
    );
    compared += 1;
  }
  assert!(compared > 0, "no YMAPs were eligible for comparison");
  eprintln!(
    "Compared {compared} YMAPs; skipped {codewalker_errors} CodeWalker XMLs containing <error> nodes."
  );
}

#[test]
#[ignore]
fn export_native_xml_for_codewalker_error_cases() {
  let input_dir = Path::new("asset/extracted");
  let output_dir = Path::new("asset/native-error-output");
  let error_case_names = [
    "cfx-gabz-mapdata___hei_kt1_occl_00.ymap",
    "cfx-gabz-mapdata___lr_sc1_occl_02.ymap",
    "cfx-gabz-mapdata___sp1_occl_01.ymap",
    "cfx-gabz-mapdata___vb_occl_01.ymap",
    "tstudio_ammunation___hei_dt1_22_strm_0.ymap",
    "tstudio_cayo_lagoon___h4_islandairstrip.ymap",
    "tstudio_laundromat___ch1_occl_02.ymap",
    "tstudio_laundromat___lr_sc1_18_strm_0.ymap",
  ];
  std::fs::create_dir_all(output_dir).unwrap();

  let mut shared_names = std::collections::HashMap::new();
  for entry in std::fs::read_dir(input_dir).unwrap() {
    let path = entry.unwrap().path();
    if path.extension().is_some_and(|extension| extension == "ymap") {
      let bytes = std::fs::read(&path).unwrap();
      let resource = Rsc7Resource::decode(&bytes).unwrap();
      shared_names.extend(MetaResource::parse(&resource).unwrap().hash_names());
    }
  }

  for name in error_case_names {
    let input = input_dir.join(name);
    let bytes = std::fs::read(&input).unwrap();
    let xml = ymap_to_xml(&bytes, &shared_names)
      .unwrap_or_else(|error| panic!("native conversion failed for {}: {error}", input.display()));
    let output = output_dir.join(format!("{name}.xml"));
    std::fs::write(&output, xml).unwrap();
    eprintln!("Wrote {}", output.display());
  }
}

fn truncate(value: &str) -> &str {
  value.get(..value.floor_char_boundary(240)).unwrap_or(value)
}

fn canonical_xml(xml: &str) -> Vec<String> {
  let mut normalized = Vec::new();
  let mut reader = Reader::from_str(xml);
  reader.config_mut().trim_text(true);
  loop {
    match reader.read_event().unwrap() {
      Event::Start(element) => {
        normalized.push(format!("start:{}", canonical_element(&reader, &element)))
      }
      Event::Empty(element) => {
        normalized.push(format!("empty:{}", canonical_element(&reader, &element)))
      }
      Event::End(element) => {
        normalized.push(format!("end:{}", String::from_utf8_lossy(element.name().as_ref())));
      }
      Event::Text(text) => {
        let value = text
          .decode()
          .unwrap()
          .split_whitespace()
          .map(normalize_value)
          .collect::<Vec<_>>()
          .join(" ");
        if !value.is_empty() {
          normalized.push(format!("text:{value}"));
        }
      }
      Event::Eof => break,
      _ => {}
    }
  }
  normalized
}

fn canonical_element(
  reader: &Reader<&[u8]>,
  element: &quick_xml::events::BytesStart<'_>,
) -> String {
  let mut attributes = element
    .attributes()
    .map(|attribute| {
      let attribute = attribute.unwrap();
      let name = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
      let value = normalize_value(&attribute.decode_and_unescape_value(reader.decoder()).unwrap());
      (name, value)
    })
    .collect::<Vec<_>>();
  attributes.sort();
  format!(
    "{}{}",
    String::from_utf8_lossy(element.name().as_ref()),
    attributes.into_iter().map(|(name, value)| format!(" {name}={value}")).collect::<String>()
  )
}

fn normalize_value(value: &str) -> String {
  if let Some(hash) = value.strip_prefix("hash_")
    && let Ok(hash) = u32::from_str_radix(hash, 16)
  {
    return format!("hash:{hash:08X}");
  }
  if let Ok(number) = value.parse::<f32>() {
    if value.contains(['.', 'e', 'E']) || value.len() > 15 {
      return format!("f32:{:08X}", number.to_bits());
    }
    return value.to_string();
  }
  format!("hash:{:08X}", jenk_hash(value))
}

fn jenk_hash(value: &str) -> u32 {
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
