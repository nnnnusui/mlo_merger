//! Round-trip smoke test for the CodeWalker.Bridge hosting layer.
//!
//! Ignored by default since it requires a locally built `bridge/CodeWalker.Bridge`
//! (see build.rs / CODEWALKER_CORE_DLL). Run with:
//!   CODEWALKER_CORE_DLL=/path/to/CodeWalker.Core.dll cargo test --test codewalker_roundtrip -- --ignored

use std::{collections::BTreeMap, path::Path};

use mlo_merger::core::codewalker::CodeWalker;
use mlo_merger::core::format::gamefile::{
  meta_resource::{MetaResource, MetaSchemaCatalog},
  meta_xml::ymap_to_xml,
  resource_convert::{NativeResourceFormat, resource_to_xml, xml_to_resource},
  resource_file::Rsc7Resource,
  xml_meta_builder::meta_from_xml,
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
  for (index, (actual_event, expected_event)) in
    actual_events.iter().zip(&expected_events).enumerate()
  {
    if actual_event != expected_event {
      let actual_context =
        actual_events[index.saturating_sub(4)..(index + 4).min(actual_events.len())].join(" | ");
      let expected_context = expected_events
        [index.saturating_sub(4)..(index + 4).min(expected_events.len())]
        .join(" | ");
      panic!(
        "rebuilt YMAP differs from source XML at event {index}: {}; actual=[{}]; expected=[{}]",
        describe_difference(actual_event, expected_event),
        actual_context,
        expected_context
      );
    }
  }
  assert_eq!(
    actual_events.len(),
    expected_events.len(),
    "rebuilt YMAP has a different XML event count"
  );
}

#[test]
#[ignore]
fn native_ybn_ymt_ynd_ytyp_results_match_codewalker_where_supported() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let bridge_dll = std::env::var("CODEWALKER_BRIDGE_DLL")
    .unwrap_or_else(|_| "bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll".to_string());
  let codewalker = CodeWalker::init(Path::new(&bridge_dll)).expect("failed to init CodeWalker");
  let temp_dir =
    std::env::temp_dir().join(format!("mlo_merger_resource_diff_{}", std::process::id()));
  std::fs::create_dir_all(&temp_dir).unwrap();

  let cases = [
    (
      NativeResourceFormat::Ybn,
      base.join("asset/source/wxmaps_lshospital_v/stream/dt1_01_0.ybn"),
      "dt1_01_0.ybn",
    ),
    (
      NativeResourceFormat::YmtRsc,
      base.join("asset/source/[nteam]/cfx-nteam-acrt/SCENARIO/elysian_island.ymt"),
      "elysian_island.ymt",
    ),
    (
      NativeResourceFormat::Ynd,
      base.join("asset/source/[nteam]/cfx-nteam-road-connection/stream/ynd/nodes752.ynd"),
      "nodes752.ynd",
    ),
    (
      NativeResourceFormat::Ytyp,
      base.join("asset/source/sb_trainheistmap/stream/sb_train_addonprops.ytyp"),
      "sb_train_addonprops.ytyp",
    ),
  ];

  let mut failures = Vec::new();
  for (format, input, name) in cases {
    let result = compare_resource_with_codewalker(&codewalker, format, &input, name, &temp_dir);
    match result {
      Ok(()) => eprintln!("{}: Native XML and rebuilt binary match CodeWalker", input.display()),
      Err(message)
        if format == NativeResourceFormat::Ybn
          && message.contains("YBN Bounds resource graph adapter is not implemented") =>
      {
        eprintln!(
          "{}: CodeWalker export succeeded; Native conversion is explicitly unsupported",
          input.display()
        );
      }
      Err(message) => failures.push(format!("{}: {message}", input.display())),
    }
  }
  std::fs::remove_dir_all(temp_dir).unwrap();
  assert!(failures.is_empty(), "CodeWalker differential mismatches:\n{}", failures.join("\n"));
}

#[test]
#[ignore]
fn native_ybn_matches_codewalker_active_source_corpus() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let bridge_dll = std::env::var("CODEWALKER_BRIDGE_DLL")
    .unwrap_or_else(|_| "bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll".to_string());
  let codewalker = CodeWalker::init(Path::new(&bridge_dll)).expect("failed to init CodeWalker");
  let source = base.join("asset/source");
  let filter = std::env::var("YBN_COMPARE_FILTER").ok();
  let input_paths = walkdir::WalkDir::new(&source)
    .follow_links(false)
    .into_iter()
    .filter_map(Result::ok)
    .filter(|entry| entry.file_type().is_file())
    .map(|entry| entry.into_path())
    .filter(|path| {
      path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("ybn"))
        && !path
          .components()
          .any(|component| matches!(component.as_os_str().to_str(), Some("_backup" | "_omit")))
        && filter.as_ref().is_none_or(|filter| path.to_string_lossy().contains(filter))
    })
    .collect::<Vec<_>>();
  let temp_dir = std::env::temp_dir().join(format!("mlo_merger_ybn_corpus_{}", std::process::id()));
  std::fs::create_dir_all(&temp_dir).unwrap();
  let mut failures = Vec::new();
  for (index, input) in input_paths.iter().enumerate() {
    let result = compare_resource_with_codewalker(
      &codewalker,
      NativeResourceFormat::Ybn,
      input,
      &format!("ybn_{index}.ybn"),
      &temp_dir,
    );
    if let Err(error) = result {
      failures.push(format!("{}: {error}", input.display()));
      if failures.len() >= 20 {
        break;
      }
    }
  }
  std::fs::remove_dir_all(temp_dir).unwrap();
  assert!(
    failures.is_empty(),
    "YBN differential failures among {} active source resources:\n{}",
    input_paths.len(),
    failures.join("\n")
  );
  eprintln!("Compared {} active source YBN resources with CodeWalker.", input_paths.len());
}

fn compare_resource_with_codewalker(
  codewalker: &CodeWalker,
  format: NativeResourceFormat,
  input: &Path,
  name: &str,
  temp_dir: &Path,
) -> Result<(), String> {
  let reference_xml_path = temp_dir.join(format!("{name}.reference.xml"));
  codewalker.game_file_to_xml(input, &reference_xml_path).map_err(|error| error.to_string())?;
  let expected_xml =
    std::fs::read_to_string(&reference_xml_path).map_err(|error| error.to_string())?;
  let bytes = std::fs::read(input).map_err(|error| error.to_string())?;
  let mut catalog = MetaSchemaCatalog::default();
  if matches!(format, NativeResourceFormat::YmtRsc | NativeResourceFormat::Ytyp) {
    let resource = Rsc7Resource::decode(&bytes).map_err(|error| error.to_string())?;
    catalog.add_resource(&MetaResource::parse(&resource).map_err(|error| error.to_string())?);
  }
  let native_xml =
    resource_to_xml(format, &bytes, &catalog.hash_names).map_err(|error| error.to_string())?;
  assert_canonical_xml_eq(
    &native_xml,
    &expected_xml,
    &format!("Native export {}", input.display()),
  )?;

  let mut rebuilt_reference_xml = expected_xml.clone();
  let codewalker_binary_path =
    (format == NativeResourceFormat::Ybn).then(|| temp_dir.join(format!("{name}.codewalker.ybn")));
  if let Some(codewalker_binary_path) = &codewalker_binary_path {
    codewalker
      .game_file_from_xml(&reference_xml_path, codewalker_binary_path)
      .map_err(|error| error.to_string())?;
    let codewalker_xml_path = temp_dir.join(format!("{name}.codewalker.xml"));
    codewalker
      .game_file_to_xml(codewalker_binary_path, &codewalker_xml_path)
      .map_err(|error| error.to_string())?;
    rebuilt_reference_xml =
      std::fs::read_to_string(codewalker_xml_path).map_err(|error| error.to_string())?;
  }

  let native_binary =
    xml_to_resource(format, &native_xml, &catalog).map_err(|error| error.to_string())?;
  if let Some(codewalker_binary_path) = &codewalker_binary_path {
    let codewalker_binary =
      std::fs::read(codewalker_binary_path).map_err(|error| error.to_string())?;
    let native_quantums = ybn_resource_quantums(&native_binary)?;
    let codewalker_quantums = ybn_resource_quantums(&codewalker_binary)?;
    let rebuilt_path = temp_dir.join(name);
    std::fs::write(&rebuilt_path, native_binary).map_err(|error| error.to_string())?;
    let rebuilt_xml_path = temp_dir.join(format!("{name}.rebuilt.xml"));
    codewalker
      .game_file_to_xml(&rebuilt_path, &rebuilt_xml_path)
      .map_err(|error| error.to_string())?;
    let rebuilt_xml =
      std::fs::read_to_string(rebuilt_xml_path).map_err(|error| error.to_string())?;
    return assert_ybn_xml_eq(
      &rebuilt_xml,
      &rebuilt_reference_xml,
      &format!("Native rebuild {}", input.display()),
      true,
      Some((&native_quantums, &codewalker_quantums)),
    );
  }
  let rebuilt_path = temp_dir.join(name);
  std::fs::write(&rebuilt_path, native_binary).map_err(|error| error.to_string())?;
  let rebuilt_xml_path = temp_dir.join(format!("{name}.rebuilt.xml"));
  codewalker
    .game_file_to_xml(&rebuilt_path, &rebuilt_xml_path)
    .map_err(|error| error.to_string())?;
  let rebuilt_xml = std::fs::read_to_string(rebuilt_xml_path).map_err(|error| error.to_string())?;
  assert_canonical_xml_eq(
    &rebuilt_xml,
    &expected_xml,
    &format!("Native rebuild {}", input.display()),
  )?;
  Ok(())
}

fn ybn_resource_quantums(bytes: &[u8]) -> Result<Vec<[f32; 3]>, String> {
  let resource = Rsc7Resource::decode(bytes).map_err(|error| error.to_string())?;
  fn visit(
    resource: &Rsc7Resource,
    pointer: u64,
    output: &mut Vec<[f32; 3]>,
  ) -> Result<(), String> {
    if pointer == 0 {
      return Ok(());
    }
    let common = resource.read_address(pointer, 112).map_err(|error| error.to_string())?;
    match common[16] {
      10 => {
        let composite = resource.read_address(pointer, 176).map_err(|error| error.to_string())?;
        let children_pointer = u64::from_le_bytes(composite[112..120].try_into().unwrap());
        let count = u16::from_le_bytes(composite[160..162].try_into().unwrap()) as usize;
        if count > 0 {
          let children = resource
            .read_address(children_pointer, count * 8)
            .map_err(|error| error.to_string())?;
          for child in children.chunks_exact(8) {
            visit(resource, u64::from_le_bytes(child.try_into().unwrap()), output)?;
          }
        }
      }
      4 | 8 => {
        let size = if common[16] == 8 { 336 } else { 304 };
        let geometry = resource.read_address(pointer, size).map_err(|error| error.to_string())?;
        if u32::from_le_bytes(geometry[208..212].try_into().unwrap()) > 0 {
          output.push([
            f32::from_le_bytes(geometry[144..148].try_into().unwrap()),
            f32::from_le_bytes(geometry[148..152].try_into().unwrap()),
            f32::from_le_bytes(geometry[152..156].try_into().unwrap()),
          ]);
        }
      }
      _ => {}
    }
    Ok(())
  }
  let mut output = Vec::new();
  visit(&resource, 0x5000_0000, &mut output)?;
  Ok(output)
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
    for (index, (actual_event, expected_event)) in
      actual_events.iter().zip(&expected_events).enumerate()
    {
      if actual_event != expected_event {
        panic!(
          "Native XML->YMAP differs after CodeWalker re-export for {} at event {index}: {}",
          input.display(),
          describe_difference(actual_event, expected_event)
        );
      }
    }
    assert_eq!(
      actual_events.len(),
      expected_events.len(),
      "{} has a different XML event count",
      input.display()
    );
    compared += 1;
  }

  assert!(compared > 0, "no XML files were eligible for differential comparison");
  eprintln!(
    "Compared {compared} Native-built YMAPs; skipped {source_errors} source XMLs containing <error> nodes."
  );
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

fn assert_canonical_xml_eq(
  actual: &str,
  expected: &str,
  context: &str,
) -> Result<(), String> {
  if context.starts_with("Native export ") && context.ends_with(".ybn") {
    return assert_ybn_xml_eq(actual, expected, context, false, None);
  }
  let actual_events = canonical_xml(actual);
  let expected_events = canonical_xml(expected);
  for (index, (actual_event, expected_event)) in
    actual_events.iter().zip(&expected_events).enumerate()
  {
    if actual_event != expected_event {
      let start = index.saturating_sub(3);
      let token_difference = actual_event
        .split_whitespace()
        .zip(expected_event.split_whitespace())
        .enumerate()
        .find(|(_, (actual_token, expected_token))| actual_token != expected_token)
        .map(|(token_index, (actual_token, expected_token))| {
          format!(" token {token_index}: Native={actual_token}, CodeWalker={expected_token}")
        })
        .unwrap_or_default();
      let actual_context = actual_events[start..(index + 4).min(actual_events.len())]
        .iter()
        .map(|event| truncate(event))
        .collect::<Vec<_>>()
        .join(" | ");
      let expected_context = expected_events[start..(index + 4).min(expected_events.len())]
        .iter()
        .map(|event| truncate(event))
        .collect::<Vec<_>>()
        .join(" | ");
      return Err(format!(
        "{context} differs at XML event {index}:{token_difference}; Native='{}' ({}), CodeWalker='{}' ({}); Native context=[{}]; CodeWalker context=[{}]",
        truncate(actual_event),
        truncate(&xml_event_at(actual, index)),
        truncate(expected_event),
        truncate(&xml_event_at(expected, index)),
        actual_context,
        expected_context
      ));
    }
  }
  if actual_events.len() != expected_events.len() {
    return Err(format!(
      "{context} has {} XML events; CodeWalker has {}",
      actual_events.len(),
      expected_events.len()
    ));
  }
  Ok(())
}

type YbnQuantumPair<'a> = (&'a [[f32; 3]], &'a [[f32; 3]]);

fn assert_ybn_xml_eq(
  actual: &str,
  expected: &str,
  context: &str,
  ignore_polygon_order: bool,
  binary_quantums: Option<YbnQuantumPair<'_>>,
) -> Result<(), String> {
  let native = canonical_ybn_xml(actual, ignore_polygon_order)?;
  let reference = canonical_ybn_xml(expected, ignore_polygon_order)?;
  if native != reference {
    let index = native
      .iter()
      .zip(&reference)
      .position(|(a, b)| a != b)
      .unwrap_or(native.len().min(reference.len()));
    return Err(format!("{context} differs in non-vertex YBN data at canonical event {index}"));
  }

  let actual_vertices = ybn_vertex_arrays(actual)?;
  let expected_vertices = ybn_vertex_arrays(expected)?;
  let (actual_quanta, expected_quanta): YbnQuantumPair<'_> = match binary_quantums {
    Some(quantums) => quantums,
    None if !ignore_polygon_order => (&[], &[]),
    None => return Err(format!("{context} requires binary Quantum values for rebuild comparison")),
  };
  if actual_vertices.len() != expected_vertices.len() {
    return Err(format!("{context} has a different number of Vertices arrays"));
  }
  for (array_index, (native, codewalker)) in
    actual_vertices.iter().zip(&expected_vertices).enumerate()
  {
    if native.len() != codewalker.len() {
      return Err(format!("{context} Vertices array {array_index} has different lengths"));
    }
    for (index, (a, b)) in native.iter().zip(codewalker).enumerate() {
      let differs = if ignore_polygon_order {
        let quantum = actual_quanta
          .get(array_index)
          .ok_or_else(|| format!("{context} lacks Native geometry quantum"))?;
        let reference_quantum = expected_quanta
          .get(array_index)
          .ok_or_else(|| format!("{context} lacks CodeWalker geometry quantum"))?;
        let epsilon = f32::EPSILON * a.abs().max(b.abs()).max(1.0) * 2.0;
        (a - b).abs() > quantum[index % 3].max(reference_quantum[index % 3]) + epsilon
      } else {
        a.to_bits() != b.to_bits()
      };
      if differs {
        let native_quantum = actual_quanta.get(array_index).copied().unwrap_or([0.0; 3]);
        let codewalker_quantum = expected_quanta.get(array_index).copied().unwrap_or([0.0; 3]);
        return Err(format!(
          "{context} Vertices array {array_index} coordinate {index} differs: Native={a}, CodeWalker={b}; quantum Native={native_quantum:?}, CodeWalker={codewalker_quantum:?}; coordinate counts Native={}, CodeWalker={}",
          native.len(),
          codewalker.len()
        ));
      }
    }
  }
  Ok(())
}

fn ybn_vertex_arrays(xml: &str) -> Result<Vec<Vec<f32>>, String> {
  let mut reader = Reader::from_str(xml);
  reader.config_mut().trim_text(true);
  let mut inside_vertices = false;
  let mut values = Vec::new();
  let mut arrays = Vec::new();
  loop {
    match reader.read_event().map_err(|error| error.to_string())? {
      Event::Start(element) if element.name().as_ref() == b"Vertices" => {
        inside_vertices = true;
        values.clear();
      }
      Event::Text(text) if inside_vertices => {
        let decoded = text.decode().map_err(|error| error.to_string())?;
        for token in decoded
          .split(|character: char| character.is_whitespace() || character == ',')
          .filter(|token| !token.is_empty())
        {
          values.push(
            token.parse::<f32>().map_err(|_| format!("invalid YBN vertex coordinate {token}"))?,
          );
        }
      }
      Event::End(element) if element.name().as_ref() == b"Vertices" => {
        arrays.push(std::mem::take(&mut values));
        inside_vertices = false;
      }
      Event::Eof => break,
      _ => {}
    }
  }
  Ok(arrays)
}

#[derive(Default)]
struct CompareXmlNode {
  name: String,
  attributes: BTreeMap<String, String>,
  text: String,
  children: Vec<CompareXmlNode>,
}

fn canonical_ybn_xml(
  xml: &str,
  ignore_polygon_order: bool,
) -> Result<Vec<String>, String> {
  let root = parse_compare_xml(xml)?;
  Ok(vec![canonical_ybn_node(&root, ignore_polygon_order)?])
}

fn parse_compare_xml(xml: &str) -> Result<CompareXmlNode, String> {
  let mut reader = Reader::from_str(xml);
  reader.config_mut().trim_text(true);
  let mut stack = Vec::<CompareXmlNode>::new();
  let mut root = None;
  loop {
    match reader.read_event().map_err(|error| error.to_string())? {
      Event::Start(element) => stack.push(compare_xml_element(&reader, &element)?),
      Event::Empty(element) => {
        attach_compare_node(compare_xml_element(&reader, &element)?, &mut stack, &mut root)?
      }
      Event::Text(text) => {
        if let Some(node) = stack.last_mut() {
          node.text.push_str(&text.decode().map_err(|error| error.to_string())?);
        }
      }
      Event::End(_) => {
        let node = stack.pop().ok_or_else(|| "invalid YBN XML nesting".to_string())?;
        attach_compare_node(node, &mut stack, &mut root)?;
      }
      Event::Eof => break,
      _ => {}
    }
  }
  root.ok_or_else(|| "YBN XML has no root element".into())
}

fn compare_xml_element(
  reader: &Reader<&[u8]>,
  element: &quick_xml::events::BytesStart<'_>,
) -> Result<CompareXmlNode, String> {
  let mut node = CompareXmlNode {
    name: String::from_utf8_lossy(element.name().as_ref()).into_owned(),
    ..CompareXmlNode::default()
  };
  for attribute in element.attributes() {
    let attribute = attribute.map_err(|error| error.to_string())?;
    node.attributes.insert(
      String::from_utf8_lossy(attribute.key.as_ref()).into_owned(),
      attribute
        .decode_and_unescape_value(reader.decoder())
        .map_err(|error| error.to_string())?
        .into_owned(),
    );
  }
  Ok(node)
}

fn attach_compare_node(
  node: CompareXmlNode,
  stack: &mut [CompareXmlNode],
  root: &mut Option<CompareXmlNode>,
) -> Result<(), String> {
  if let Some(parent) = stack.last_mut() {
    parent.children.push(node);
  } else if root.replace(node).is_some() {
    return Err("YBN XML has multiple root elements".into());
  }
  Ok(())
}

fn canonical_ybn_node(
  node: &CompareXmlNode,
  ignore_polygon_order: bool,
) -> Result<String, String> {
  if node.name == "Vertices" {
    return Ok("Vertices<quantized>".into());
  }
  let material_keys = node
    .children
    .iter()
    .find(|child| child.name == "Materials")
    .map(|materials| {
      materials
        .children
        .iter()
        .filter(|item| item.name == "Item")
        .map(canonical_compare_node)
        .collect::<Vec<_>>()
    })
    .unwrap_or_default();
  let mut children = Vec::new();
  for child in &node.children {
    match child.name.as_str() {
      "Vertices" => children.push("Vertices<quantized>".into()),
      "Materials" => {
        let mut values = material_keys.clone();
        values.sort();
        children.push(format!("Materials[{}]", values.join("|")));
      }
      "Polygons" => {
        let mut polygons = Vec::new();
        for polygon in &child.children {
          let material_index = polygon
            .attributes
            .get("m")
            .ok_or_else(|| "YBN polygon has no material index".to_string())?
            .parse::<usize>()
            .map_err(|_| "YBN polygon material index is invalid".to_string())?;
          let material = material_keys.get(material_index).ok_or_else(|| {
            format!("YBN polygon material index {material_index} is out of range")
          })?;
          polygons.push(canonical_compare_node_without_material_index(polygon, material));
        }
        if ignore_polygon_order {
          polygons.sort();
        }
        children.push(format!("Polygons[{}]", polygons.join("|")));
      }
      _ => children.push(canonical_ybn_node(child, ignore_polygon_order)?),
    }
  }
  let mut attributes = node
    .attributes
    .iter()
    .map(|(key, value)| format!("{key}={}", normalize_value(value)))
    .collect::<Vec<_>>();
  attributes.sort();
  let text = node
    .text
    .split(|character: char| character.is_whitespace() || character == ',')
    .filter(|part| !part.is_empty())
    .map(normalize_value)
    .collect::<Vec<_>>()
    .join(" ");
  Ok(format!(
    "{}({})[{}]{{{}}}",
    normalize_element_name(&node.name),
    attributes.join(" "),
    text,
    children.join(";")
  ))
}

fn canonical_compare_node(node: &CompareXmlNode) -> String {
  canonical_compare_node_without_material_index(node, "")
}

fn canonical_compare_node_without_material_index(
  node: &CompareXmlNode,
  material: &str,
) -> String {
  let mut attributes = node
    .attributes
    .iter()
    .filter(|(key, _)| key.as_str() != "m")
    .map(|(key, value)| format!("{key}={}", normalize_value(value)))
    .collect::<Vec<_>>();
  attributes.sort();
  let text = node
    .text
    .split(|character: char| character.is_whitespace() || character == ',')
    .filter(|part| !part.is_empty())
    .map(normalize_value)
    .collect::<Vec<_>>()
    .join(" ");
  format!(
    "{}({})[{}]{{{}}}<material:{material}>",
    normalize_element_name(&node.name),
    attributes.join(" "),
    text,
    node.children.iter().map(canonical_compare_node).collect::<Vec<_>>().join(";")
  )
}

fn xml_event_at(
  xml: &str,
  target: usize,
) -> String {
  let mut reader = Reader::from_str(xml);
  reader.config_mut().trim_text(true);
  let mut index = 0;
  let mut stack = Vec::new();
  loop {
    let event = match reader.read_event() {
      Ok(event) => event,
      Err(error) => return format!("XML parse error: {error}"),
    };
    let label = match event {
      Event::Start(element) => {
        let name = String::from_utf8_lossy(element.name().as_ref()).into_owned();
        let label = format!("start:{}/{}", stack.join("/"), name);
        stack.push(name);
        Some(label)
      }
      Event::Empty(element) => Some(format!(
        "empty:{}/{}",
        stack.join("/"),
        String::from_utf8_lossy(element.name().as_ref())
      )),
      Event::End(element) => {
        let name = String::from_utf8_lossy(element.name().as_ref()).into_owned();
        let label = format!("end:{}/{}", stack.join("/"), name);
        stack.pop();
        Some(label)
      }
      Event::Text(text) => {
        let value = text.decode().unwrap_or_default();
        let value = value.trim();
        (!value.is_empty()).then(|| format!("text:{}/{}", stack.join("/"), value))
      }
      Event::Eof => return "EOF".to_string(),
      _ => None,
    };
    if let Some(label) = label {
      if index == target {
        return label;
      }
      index += 1;
    }
  }
}

fn describe_difference(
  actual: &str,
  expected: &str,
) -> String {
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
        let name = normalize_element_name(&String::from_utf8_lossy(element.name().as_ref()));
        let start_prefix = format!("start:{name}");
        if normalized.last().is_some_and(|event| {
          event == &start_prefix || event.starts_with(&format!("{start_prefix} "))
        }) {
          let start = normalized.pop().unwrap();
          normalized.push(start.replacen("start:", "empty:", 1));
        } else {
          normalized.push(format!("end:{name}"));
        }
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
  format!("hash:{:08X}", jenk_hash(name))
}

fn normalize_value(value: &str) -> String {
  let value = value.trim_end_matches(',');
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
