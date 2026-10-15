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
