//! Round-trip smoke test for the CodeWalker.Bridge hosting layer.
//!
//! Ignored by default since it requires a locally built `bridge/CodeWalker.Bridge`
//! (see build.rs / CODEWALKER_CORE_DLL). Run with:
//!   CODEWALKER_CORE_DLL=/path/to/CodeWalker.Core.dll cargo test --test codewalker_roundtrip -- --ignored

use std::path::Path;

#[path = "../src/core/format/gamefile/test/compare.rs"]
mod compare;
use compare::{
  assert_canonical_xml_eq, assert_ybn_xml_eq, canonical_xml, describe_difference, truncate,
  ybn_resource_quantums,
};

use mlo_merger::core::codewalker::CodeWalker;
use mlo_merger::core::format::gamefile::{
  meta_resource::{MetaResource, MetaSchemaCatalog},
  meta_xml::ymap_to_xml,
  resource_convert::{NativeResourceFormat, resource_to_xml, xml_to_resource},
  resource_file::Rsc7Resource,
  xml_meta_builder::meta_from_xml,
};
use mlo_merger::core::xmlconvert::Xml2Ymap;

#[test]
#[ignore = "requires local merged XML, extracted schemas, and CodeWalker.Core.dll"]
fn native_crash_rebuilds_preserve_runtime_schemas() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let bridge = std::env::var("CODEWALKER_BRIDGE_DLL").unwrap_or_else(|_| {
    base.join("bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll").display().to_string()
  });
  let codewalker = CodeWalker::init(Path::new(&bridge)).unwrap();
  let output = base.join("asset/merged_native_fixed");
  let reports = base.join("asset/merge_validation/native_crash_fix");
  std::fs::create_dir_all(&output).unwrap();
  std::fs::create_dir_all(&reports).unwrap();
  let mut paths = walkdir::WalkDir::new(base.join("asset/extracted"))
    .into_iter()
    .map(|entry| entry.unwrap())
    .filter(|entry| entry.file_type().is_file())
    .map(|entry| entry.into_path())
    .filter(|path| path.extension().is_some_and(|extension| extension == "ymap"))
    .collect::<Vec<_>>();
  paths.sort();
  let mut catalog = MetaSchemaCatalog::default();
  for path in paths {
    let bytes = std::fs::read(path).unwrap();
    if let Ok(resource) = Rsc7Resource::decode(&bytes) {
      catalog.add_resource(&MetaResource::parse(&resource).unwrap());
    }
  }
  for name in ["bkr_id1_09", "h4_mph4_terrain_02_grass_0"] {
    let xml_path = base.join(format!("asset/merged.xml/{name}.ymap.xml"));
    let xml = std::fs::read_to_string(&xml_path).unwrap();
    let native_bytes = xml_to_resource(NativeResourceFormat::Ymap, &xml, &catalog).unwrap();
    let native_path = output.join(format!("{name}.ymap"));
    std::fs::write(&native_path, &native_bytes).unwrap();
    let reference_path = reports.join(format!("{name}.dll.ymap"));
    codewalker.xml_to_ymap(&xml_path, &reference_path).unwrap();
    let native_resource = Rsc7Resource::decode(&native_bytes).unwrap();
    let native = MetaResource::parse(&native_resource).unwrap();
    let root = native_resource.read_address(0x5000_0000, 0x70).unwrap();
    let pages_pointer = u64::from_le_bytes(root[8..16].try_into().unwrap());
    let pages = native_resource.read_address(pages_pointer, 16).unwrap();
    assert_eq!(pages[8], 1, "Native contiguous system must occupy one actual page");
    assert_eq!(pages[9], 0);
    let table_pointer = u64::from_le_bytes(root[48..56].try_into().unwrap());
    let table = native_resource.read_address(table_pointer, native.data_blocks.len() * 16).unwrap();
    for record in table.chunks_exact(16) {
      let size = u32::from_le_bytes(record[4..8].try_into().unwrap()) as usize;
      let pointer = u64::from_le_bytes(record[8..16].try_into().unwrap());
      if size != 0 {
        assert!(pointer >= 0x5000_0000);
        assert!(
          (pointer - 0x5000_0000) as usize + size <= native_resource.system_data.len(),
          "META block extends outside its single page"
        );
      }
    }
    let reference_bytes = std::fs::read(&reference_path).unwrap();
    let reference = MetaResource::parse(&Rsc7Resource::decode(&reference_bytes).unwrap()).unwrap();
    for expected in &reference.structures {
      let actual =
        native.structures.iter().find(|schema| schema.name_hash == expected.name_hash).unwrap();
      assert_eq!(
        actual, expected,
        "runtime structure metadata differs for {name}, type {:08X}",
        expected.name_hash
      );
    }
    for expected in &reference.enums {
      let actual =
        native.enums.iter().find(|schema| schema.name_hash == expected.name_hash).unwrap();
      assert_eq!(
        actual, expected,
        "runtime enum metadata differs for {name}, type {:08X}",
        expected.name_hash
      );
    }
    let native_xml_path = reports.join(format!("{name}.native.ymap.xml"));
    let reference_xml_path = reports.join(format!("{name}.dll.ymap.xml"));
    codewalker.ymap_to_xml(&native_path, &native_xml_path).unwrap();
    codewalker.ymap_to_xml(&reference_path, &reference_xml_path).unwrap();
    assert_canonical_xml_eq(
      &std::fs::read_to_string(native_xml_path).unwrap(),
      &std::fs::read_to_string(reference_xml_path).unwrap(),
      name,
    )
    .unwrap();
    eprintln!(
      "{name}: runtime schemas and re-exported XML match DLL; fixed Native binary: {}",
      native_path.display()
    );
  }
}

#[test]
#[ignore = "prepares local in-game crash comparison files using CodeWalker.Core.dll"]
fn rebuild_crashing_merged_ymaps_with_codewalker() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let bridge = std::env::var("CODEWALKER_BRIDGE_DLL").unwrap_or_else(|_| {
    base.join("bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll").display().to_string()
  });
  let codewalker = CodeWalker::init(Path::new(&bridge)).unwrap();
  let binary_dir = base.join("asset/merged_codewalker_dll");
  let report_dir = base.join("asset/merge_validation/crash_rebuild");
  std::fs::create_dir_all(&binary_dir).unwrap();
  std::fs::create_dir_all(&report_dir).unwrap();
  let mut records = Vec::new();
  for name in ["h4_mph4_terrain_02_grass_0", "bkr_id1_09"] {
    let input = base.join(format!("asset/merged.xml/{name}.ymap.xml"));
    let binary_path = binary_dir.join(format!("{name}.ymap"));
    codewalker.xml_to_ymap(&input, &binary_path).unwrap();
    let dll_xml_path = report_dir.join(format!("{name}.dll.ymap.xml"));
    codewalker.ymap_to_xml(&binary_path, &dll_xml_path).unwrap();
    let dll_xml = std::fs::read_to_string(dll_xml_path).unwrap();
    assert!(!dll_xml.contains("<error"), "DLL rebuild exports error nodes for {name}");
    let dll_bytes = std::fs::read(&binary_path).unwrap();
    let mut variants = serde_json::Map::new();
    for (label, directory) in [
      ("native", "asset/merged"),
      ("gui", "asset/merged_codewalker_gui"),
      ("dll", "asset/merged_codewalker_dll"),
    ] {
      let path = base.join(directory).join(format!("{name}.ymap"));
      let bytes = std::fs::read(&path).unwrap();
      let resource = Rsc7Resource::decode(&bytes).unwrap();
      let meta = MetaResource::parse(&resource).unwrap();
      let xml_path = report_dir.join(format!("{name}.{label}.ymap.xml"));
      codewalker.ymap_to_xml(&path, &xml_path).unwrap();
      let xml = std::fs::read_to_string(xml_path).unwrap();
      let difference = assert_canonical_xml_eq(&xml, &dll_xml, label).err();
      variants.insert(
        label.into(),
        serde_json::json!({
          "path": path.strip_prefix(base).unwrap().to_string_lossy(),
          "byte_length": bytes.len(),
          "byte_identical_to_dll": bytes == dll_bytes,
          "xml_matches_dll": difference.is_none(),
          "xml_difference": difference,
          "rsc_version": resource.version,
          "root_block_index": meta.root_block_index,
          "meta_name": meta.name,
          "structure_count": meta.structures.len(),
          "enum_count": meta.enums.len(),
          "data_blocks": meta.data_blocks.iter().map(|block| serde_json::json!({
            "type_hash": format!("{:08X}", block.structure_name_hash), "bytes": block.data.len(),
          })).collect::<Vec<_>>(),
        }),
      );
    }
    assert_eq!(
      variants["gui"]["byte_identical_to_dll"], true,
      "DLL rebuild differs from known-good GUI binary: {name}"
    );
    eprintln!(
      "{name}: DLL binary prepared; native XML matches={}, GUI XML matches={}",
      variants["native"]["xml_matches_dll"], variants["gui"]["xml_matches_dll"]
    );
    records.push(serde_json::json!({ "ymap": name, "input_xml": input.strip_prefix(base).unwrap().to_string_lossy(), "variants": variants }));
  }
  std::fs::write(report_dir.join("results.json"), serde_json::to_vec_pretty(&records).unwrap())
    .unwrap();
}

#[test]
#[ignore = "requires four local vanilla PSO YMAPs and CodeWalker.Core.dll"]
fn native_pso_ymap_exports_match_codewalker() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let bridge = std::env::var("CODEWALKER_BRIDGE_DLL").unwrap_or_else(|_| {
    base.join("bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll").display().to_string()
  });
  let codewalker = CodeWalker::init(Path::new(&bridge)).unwrap();
  let output = base.join("asset/merge_validation/pso_ymap");
  std::fs::create_dir_all(&output).unwrap();
  for name in ["cs1_railwyc", "cs1_railwyc_long_0", "id2_17", "id2_17_strm_0"] {
    let input = base.join(format!("asset/vanilla/ymap/{name}.ymap"));
    let reference = output.join(format!("{name}.ymap.codewalker.pso.xml"));
    codewalker.ymap_to_xml(&input, &reference).unwrap();
    let bytes = std::fs::read(&input).unwrap();
    let native = resource_to_xml(NativeResourceFormat::Ymap, &bytes, &Default::default()).unwrap();
    std::fs::write(output.join(format!("{name}.ymap.native.pso.xml")), &native).unwrap();
    let expected = std::fs::read_to_string(reference).unwrap();
    assert_canonical_xml_eq(&native, &expected, &format!("PSO export {name}")).unwrap();
    let rebuilt = mlo_merger::core::format::gamefile::pso::PsoResource::parse(&bytes)
      .unwrap()
      .rebuild_xml(&native)
      .unwrap();
    let rebuilt_path = output.join(format!("{name}.ymap"));
    std::fs::write(&rebuilt_path, rebuilt).unwrap();
    let rebuilt_xml = output.join(format!("{name}.rebuilt.pso.xml"));
    codewalker.ymap_to_xml(&rebuilt_path, &rebuilt_xml).unwrap();
    assert_canonical_xml_eq(
      &std::fs::read_to_string(rebuilt_xml).unwrap(),
      &expected,
      &format!("PSO rebuild {name}"),
    )
    .unwrap();
    eprintln!("{name}: Native PSO export matches CodeWalker");
  }
}

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
