//! Round-trip smoke test for the CodeWalker.Bridge hosting layer.
//!
//! Ignored by default since it requires a locally built `bridge/CodeWalker.Bridge`
//! (see build.rs / CODEWALKER_CORE_DLL). Run with:
//!   CODEWALKER_CORE_DLL=/path/to/CodeWalker.Core.dll cargo test --test codewalker_roundtrip -- --ignored

use std::path::Path;

use mlo_merger::core::codewalker::CodeWalker;
use mlo_merger::core::format::gamefile::{
  meta_resource::{MetaResource, MetaSchemaCatalog}, meta_xml::ymap_to_xml,
  resource_file::Rsc7Resource, xml_meta_builder::meta_from_xml,
};
use mlo_merger::core::xmlconvert::Xml2Ymap;
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
fn native_meta_rebuild_is_readable_by_codewalker() {
  let bridge_dll = std::env::var("CODEWALKER_BRIDGE_DLL")
    .unwrap_or_else(|_| "bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll".to_string());
  let codewalker = CodeWalker::init(Path::new(&bridge_dll)).expect("failed to init CodeWalker");
  let sample = Path::new("asset/extracted/brofx_mansion_06___apa_ch2_occl_05.ymap");
  let temp_dir = std::env::temp_dir().join("mlo_merger_native_meta_rebuild");
  std::fs::create_dir_all(&temp_dir).unwrap();
  let original_xml_path = temp_dir.join("original.ymap.xml");
  let rebuilt_binary_path = temp_dir.join("rebuilt.ymap");
  let rebuilt_xml_path = temp_dir.join("rebuilt.ymap.xml");

  codewalker.ymap_to_xml(sample, &original_xml_path).unwrap();
  let original_bytes = std::fs::read(sample).unwrap();
  let original_resource = Rsc7Resource::decode(&original_bytes).unwrap();
  let meta = MetaResource::parse(&original_resource).unwrap();
  let rebuilt_resource = meta.to_rsc7(original_resource.version).unwrap();
  std::fs::write(&rebuilt_binary_path, rebuilt_resource.encode().unwrap()).unwrap();

  codewalker.ymap_to_xml(&rebuilt_binary_path, &rebuilt_xml_path).unwrap();
  let expected = std::fs::read_to_string(original_xml_path).unwrap();
  let actual = std::fs::read_to_string(rebuilt_xml_path).unwrap();
  assert_eq!(canonical_xml(&actual), canonical_xml(&expected));
}

#[test]
#[ignore]
fn native_xml_rebuild_is_readable_by_codewalker() {
  let bridge_dll = std::env::var("CODEWALKER_BRIDGE_DLL")
    .unwrap_or_else(|_| "bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll".to_string());
  let codewalker = CodeWalker::init(Path::new(&bridge_dll)).expect("failed to init CodeWalker");
  let input = Path::new("asset/extracted/brofx_mansion_06___apa_ch2_occl_05.ymap");
  let reference_xml = Path::new("asset/extracted.xml/brofx_mansion_06___apa_ch2_occl_05.ymap.xml");
  let temp_dir = std::env::temp_dir().join("mlo_merger_native_xml_rebuild");
  std::fs::create_dir_all(&temp_dir).unwrap();
  let rebuilt_binary_path = temp_dir.join("rebuilt.ymap");
  let rebuilt_xml_path = temp_dir.join("rebuilt.ymap.xml");

  let source_bytes = std::fs::read(input).unwrap();
  let source_resource = Rsc7Resource::decode(&source_bytes).unwrap();
  let schema_source = MetaResource::parse(&source_resource).unwrap();
  let xml = std::fs::read_to_string(reference_xml).unwrap();
  let mut catalog = MetaSchemaCatalog::default();
  catalog.add_resource(&schema_source);
  let rebuilt_meta = meta_from_xml(&xml, &catalog).unwrap();
  let rebuilt_resource = rebuilt_meta.to_rsc7(source_resource.version).unwrap();
  std::fs::write(&rebuilt_binary_path, rebuilt_resource.encode().unwrap()).unwrap();

  codewalker.ymap_to_xml(&rebuilt_binary_path, &rebuilt_xml_path).unwrap();
  let actual = std::fs::read_to_string(rebuilt_xml_path).unwrap();
  let actual_events = canonical_xml(&actual);
  let expected_events = canonical_xml(&xml);
  for (index, (actual_event, expected_event)) in actual_events.iter().zip(&expected_events).enumerate() {
    if actual_event != expected_event {
      let actual_context = actual_events[index.saturating_sub(4)..(index + 4).min(actual_events.len())]
        .join(" | ");
      let expected_context =
        expected_events[index.saturating_sub(4)..(index + 4).min(expected_events.len())].join(" | ");
      panic!(
        "rebuilt YMAP differs from source XML at event {index}: {}; actual=[{}]; expected=[{}]",
        describe_difference(actual_event, expected_event),
        actual_context,
        expected_context
      );
    }
  }
  assert_eq!(actual_events.len(), expected_events.len(), "rebuilt YMAP has a different XML event count");
}

#[test]
#[ignore]
fn native_xml_to_ymap_rebuilds_full_extracted_xml_corpus() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let input_dir = base.join("asset/extracted.xml");
  let schema_dir = base.join("asset/extracted");
  let output_dir = std::env::temp_dir().join("mlo_merger_native_xml_corpus");
  std::fs::create_dir_all(&output_dir).unwrap();

  Xml2Ymap {
    input_dir: input_dir.clone(),
    output_dir: output_dir.clone(),
  }
  .run_native(&schema_dir)
  .unwrap();

  let inputs = std::fs::read_dir(input_dir)
    .unwrap()
    .map(|entry| entry.unwrap().path())
    .filter(|path| path.extension().is_some_and(|extension| extension == "xml"))
    .collect::<Vec<_>>();
  for input in &inputs {
    let relative = input.strip_prefix(base.join("asset/extracted.xml")).unwrap();
    let output = output_dir.join(relative).with_extension("");
    assert!(output.is_file(), "Native conversion did not create {}", output.display());
    let bytes = std::fs::read(&output).unwrap();
    Rsc7Resource::decode(&bytes)
      .unwrap_or_else(|error| panic!("invalid native RSC7 output {}: {error}", output.display()));
  }
  eprintln!("Native-converted {} extracted XML files to RSC7 YMAPs.", inputs.len());
}

#[test]
#[ignore]
fn native_xml_to_ymap_matches_codewalker_corpus() {
  let bridge_dll = std::env::var("CODEWALKER_BRIDGE_DLL")
    .unwrap_or_else(|_| "bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll".to_string());
  let codewalker = CodeWalker::init(Path::new(&bridge_dll)).expect("failed to init CodeWalker");
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let input_dir = base.join("asset/extracted.xml");
  let schema_dir = base.join("asset/extracted");
  let output_dir = std::env::temp_dir().join("mlo_merger_native_xml_compare");
  let roundtrip_dir = output_dir.join("codewalker-xml");
  std::fs::create_dir_all(&roundtrip_dir).unwrap();

  Xml2Ymap {
    input_dir: input_dir.clone(),
    output_dir: output_dir.join("native-ymap"),
  }
  .run_native(&schema_dir)
  .unwrap();

  let binary_inputs = std::fs::read_dir(&schema_dir)
    .unwrap()
    .map(|entry| entry.unwrap().path())
    .filter(|path| path.extension().is_some_and(|extension| extension == "ymap"))
    .collect::<Vec<_>>();
  for input in &binary_inputs {
    codewalker.preload_names(input).unwrap();
  }

  let xml_inputs = std::fs::read_dir(&input_dir)
    .unwrap()
    .map(|entry| entry.unwrap().path())
    .filter(|path| path.extension().is_some_and(|extension| extension == "xml"))
    .collect::<Vec<_>>();
  let native_dir = output_dir.join("native-ymap");
  let mut compared = 0usize;
  let mut source_errors = 0usize;
  for input in &xml_inputs {
    let source_xml = std::fs::read_to_string(input).unwrap();
    if source_xml.contains("<error>") {
      source_errors += 1;
      continue;
    }
    let relative = input.strip_prefix(&input_dir).unwrap();
    let native_binary = native_dir.join(relative).with_extension("");
    let roundtrip_xml = roundtrip_dir.join(relative);
    codewalker.ymap_to_xml(&native_binary, &roundtrip_xml).unwrap();
    let actual = std::fs::read_to_string(roundtrip_xml).unwrap();
    let actual_events = canonical_xml(&actual);
    let expected_events = canonical_xml(&source_xml);
    for (index, (actual_event, expected_event)) in actual_events.iter().zip(&expected_events).enumerate() {
      if actual_event != expected_event {
        panic!(
          "Native XML->YMAP differs after CodeWalker re-export for {} at event {index}: {}",
          input.display(),
          describe_difference(actual_event, expected_event)
        );
      }
    }
    assert_eq!(actual_events.len(), expected_events.len(), "{} has a different XML event count", input.display());
    compared += 1;
  }

  assert!(compared > 0, "no XML files were eligible for differential comparison");
  eprintln!("Compared {compared} Native-built YMAPs; skipped {source_errors} source XMLs containing <error> nodes.");
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
      if native_event != expected_event {
        panic!(
          "{} differs from CodeWalker at XML event {index}: native='{}', CodeWalker='{}'; native context=[{}], CodeWalker context=[{}]",
          input.display(),
          truncate(native_event),
          truncate(expected_event),
          native_context,
          expected_context
        );
      }
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

fn describe_difference(actual: &str, expected: &str) -> String {
  let offset = actual
    .chars()
    .zip(expected.chars())
    .position(|(actual, expected)| actual != expected)
    .unwrap_or_else(|| actual.len().min(expected.len()));
  let start = offset.saturating_sub(60);
  let actual_end = (offset + 100).min(actual.len());
  let expected_end = (offset + 100).min(expected.len());
  format!(
    "first text offset {offset}; native='{}', source='{}'",
    actual.get(start..actual_end).unwrap_or(actual),
    expected.get(start..expected_end).unwrap_or(expected)
  )
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
    normalize_element_name(&String::from_utf8_lossy(element.name().as_ref())),
    attributes.into_iter().map(|(name, value)| format!(" {name}={value}")).collect::<String>()
  )
}

fn normalize_element_name(name: &str) -> String {
  let reserved_type = match name {
    "STRING" => Some(0x10),
    "BYTE" => Some(0x11),
    "USHORT" => Some(0x13),
    "UINT" => Some(0x15),
    "FLOAT" => Some(0x21),
    "VECTOR4" => Some(0x33),
    "hash" => Some(0x4a),
    "POINTER" => Some(0x07),
    "ARRAYINFO" => Some(0x100),
    _ => None,
  };
  if let Some(hash) = reserved_type {
    return format!("hash:{hash:08X}");
  }
  if let Some(hash) = name.strip_prefix("hash_")
    && let Ok(hash) = u32::from_str_radix(hash, 16)
  {
    return format!("hash:{hash:08X}");
  }
  name.to_string()
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
