use super::*;

#[test]
#[ignore = "requires local merged distant-light XML and CodeWalker.Core.dll"]
fn native_merged_distant_lights_match_codewalker() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let bridge = std::env::var("CODEWALKER_BRIDGE_DLL").unwrap_or_else(|_| {
    base.join("bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll").display().to_string()
  });
  let codewalker = CodeWalker::init(Path::new(&bridge)).unwrap();
  let output = base.join("asset/merged_native_fixed");
  let reports = base.join("asset/merge_validation/distant_lights_fix");
  std::fs::create_dir_all(&output).unwrap();
  std::fs::create_dir_all(&reports).unwrap();
  let mut paths = std::fs::read_dir(base.join("asset/merged.xml"))
    .unwrap()
    .map(|entry| entry.unwrap().path())
    .filter(|path| {
      path.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
        name.starts_with("vw_distlodlights_medium") && name.ends_with(".ymap.xml")
      })
    })
    .collect::<Vec<_>>();
  paths.sort();
  assert!(!paths.is_empty(), "no merged distant-light fixtures");
  let mut results = Vec::new();
  for input in paths {
    let file_name = input.file_name().unwrap().to_str().unwrap();
    let name = file_name.strip_suffix(".xml").unwrap();
    let bytes = std::fs::read(base.join("asset/vanilla/ymap").join(name)).unwrap();
    let mut catalog = MetaSchemaCatalog::default();
    catalog.add_resource(&MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap());
    let xml = std::fs::read_to_string(&input).unwrap();
    let native = xml_to_resource(NativeResourceFormat::Ymap, &xml, &catalog).unwrap();
    let native_path = output.join(name);
    std::fs::write(&native_path, &native).unwrap();
    let dll_path = reports.join(name);
    codewalker.xml_to_ymap(&input, &dll_path).unwrap();
    let native_xml = reports.join(format!("{name}.native.xml"));
    let dll_xml = reports.join(format!("{name}.dll.xml"));
    codewalker.ymap_to_xml(&native_path, &native_xml).unwrap();
    codewalker.ymap_to_xml(&dll_path, &dll_xml).unwrap();
    assert_canonical_xml_eq(
      &std::fs::read_to_string(native_xml).unwrap(),
      &std::fs::read_to_string(dll_xml).unwrap(),
      name,
    )
    .unwrap();
    let meta = MetaResource::parse(&Rsc7Resource::decode(&native).unwrap()).unwrap();
    let float_xyz = 0xE2CB_CFD4;
    let positions = meta
      .data_blocks
      .iter()
      .filter(|block| block.structure_name_hash == float_xyz)
      .map(|block| block.data.len())
      .sum::<usize>();
    assert!(positions > 0, "distant-light position block missing: {name}");
    results.push(serde_json::json!({ "ymap": name, "position_block_bytes": positions, "native_bytes": native.len(), "dll_bytes": std::fs::metadata(dll_path).unwrap().len() }));
    eprintln!("{name}: positions retained, RGBI counts consistent, DLL XML matches");
  }
  std::fs::write(reports.join("results.json"), serde_json::to_vec_pretty(&results).unwrap())
    .unwrap();
}

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
