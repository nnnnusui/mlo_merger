use super::*;

#[test]
#[ignore = "requires local YMAP schemas and CodeWalker.Core.dll"]
fn sample_parent_relink_survives_native_and_codewalker_rebuild() {
  use mlo_merger::core::format::ymap::xml::XmlYmap;
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let staging = std::env::temp_dir().join(format!("mlo_parent_refs_binary_{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&staging);
  let samples = base.join("docs/sample/parent_refs");
  let vanilla_dir = staging.join("vanilla");
  let mod_dir = staging.join("mods");
  let mod_ymap_dir = staging.join("mod-binaries");
  let output = staging.join("output/merged.xml");
  let reports = staging.join("reports");
  std::fs::create_dir_all(&vanilla_dir).unwrap();
  std::fs::create_dir_all(&mod_dir).unwrap();
  std::fs::create_dir_all(&mod_ymap_dir).unwrap();
  std::fs::create_dir_all(&reports).unwrap();
  std::fs::copy(samples.join("vanilla_parent.ymap.xml"), vanilla_dir.join("parent.ymap.xml"))
    .unwrap();
  std::fs::copy(samples.join("child.ymap.xml"), vanilla_dir.join("dependent.ymap.xml")).unwrap();
  std::fs::copy(
    samples.join("resource_a_parent.ymap.xml"),
    mod_dir.join("resource_a___parent.ymap.xml"),
  )
  .unwrap();
  std::fs::copy(
    samples.join("resource_b_parent.ymap.xml"),
    mod_dir.join("resource_b___parent.ymap.xml"),
  )
  .unwrap();
  std::fs::copy(samples.join("child.ymap.xml"), mod_dir.join("resource_a___child.ymap.xml"))
    .unwrap();
  std::fs::write(mod_ymap_dir.join("resource_a___child.ymap"), b"placeholder clone binary")
    .unwrap();
  MergeYmap {
    vanilla_dir,
    mod_dir,
    mod_ymap_dir,
    output_dir: output.clone(),
    rebuild_all: false,
    blacklist_config: None,
  }
  .run()
  .unwrap();
  let bridge = std::env::var("CODEWALKER_BRIDGE_DLL").unwrap_or_else(|_| {
    base.join("bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll").display().to_string()
  });
  let codewalker = CodeWalker::init(Path::new(&bridge)).unwrap();
  let catalog = local_ymap_schema_catalog(base);
  assert!(
    !catalog.structures.is_empty(),
    "No local YMAP META schemas found in extracted or vanilla-cache/latest/ymap"
  );
  let mut native_maps = Vec::new();
  let mut dll_maps = Vec::new();
  for name in ["parent", "child", "dependent"] {
    let xml_path = output.join(format!("{name}.ymap.xml"));
    let xml = std::fs::read_to_string(&xml_path).unwrap();
    let source: XmlYmap = quick_xml::de::from_str(&xml).unwrap();
    let native = xml_to_resource(NativeResourceFormat::Ymap, &xml, &catalog).unwrap();
    let native_path = reports.join(format!("{name}.native.ymap"));
    let dll_path = reports.join(format!("{name}.dll.ymap"));
    std::fs::write(&native_path, native).unwrap();
    codewalker.xml_to_ymap(&xml_path, &dll_path).unwrap();
    let native_xml = reports.join(format!("{name}.native.xml"));
    let dll_xml = reports.join(format!("{name}.dll.xml"));
    codewalker.ymap_to_xml(&native_path, &native_xml).unwrap();
    codewalker.ymap_to_xml(&dll_path, &dll_xml).unwrap();
    let native_text = std::fs::read_to_string(native_xml).unwrap();
    let dll_text = std::fs::read_to_string(dll_xml).unwrap();
    let native_map: XmlYmap = quick_xml::de::from_str(&native_text).unwrap();
    let dll_map: XmlYmap = quick_xml::de::from_str(&dll_text).unwrap();
    let expected = source.entities.items.iter().map(|entity| entity.guid.value).collect::<Vec<_>>();
    for rebuilt in [&native_map, &dll_map] {
      assert_eq!(
        rebuilt.entities.items.iter().map(|entity| entity.guid.value).collect::<Vec<_>>(),
        expected,
        "entity order differs after rebuilding {name}"
      );
    }
    native_maps.push(native_map);
    dll_maps.push(dll_map);
  }
  for maps in [&native_maps, &dll_maps] {
    assert_eq!(maps[0].entities.items.len(), 1);
    assert_eq!(maps[0].entities.items[0].guid.value, 200);
    for child in [&maps[1].entities.items[0], &maps[2].entities.items[0]] {
      assert_eq!(child.parent_index.value, 0);
      assert_eq!(maps[0].entities.items[child.parent_index.value as usize].guid.value, 200);
      assert_eq!(child.flags.value & 8, 8);
    }
  }
  eprintln!(
    "Native/DLL sample children resolve parent index 0 to GUID 200; XML matches and entity order is retained"
  );
  std::fs::remove_dir_all(staging).unwrap();
}

#[test]
#[ignore]
fn ymap_xml_round_trip() {
  let bridge_dll = std::env::var("CODEWALKER_BRIDGE_DLL")
    .unwrap_or_else(|_| "bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll".to_string());
  let codewalker = CodeWalker::init(Path::new(&bridge_dll)).expect("failed to init CodeWalker");
  let sample = sample_parent_refs_dir().join("child.ymap.xml");
  let tmp_dir =
    std::env::temp_dir().join(format!("mlo_merger_codewalker_roundtrip_{}", std::process::id()));
  std::fs::create_dir_all(&tmp_dir).unwrap();
  let first_binary = tmp_dir.join("first.ymap");
  let first_xml = tmp_dir.join("first.ymap.xml");
  let second_binary = tmp_dir.join("second.ymap");
  let second_xml = tmp_dir.join("second.ymap.xml");
  codewalker.xml_to_ymap(&sample, &first_binary).unwrap();
  codewalker.ymap_to_xml(&first_binary, &first_xml).unwrap();
  codewalker.xml_to_ymap(&first_xml, &second_binary).unwrap();
  codewalker.ymap_to_xml(&second_binary, &second_xml).unwrap();
  let first: XmlYmap =
    quick_xml::de::from_str(&std::fs::read_to_string(first_xml).unwrap()).unwrap();
  let second: XmlYmap =
    quick_xml::de::from_str(&std::fs::read_to_string(second_xml).unwrap()).unwrap();
  assert_sample_entity_refs(&first, &second, "CodeWalker XML/binary roundtrip");
  std::fs::remove_dir_all(tmp_dir).unwrap();
}

#[test]
#[ignore]
fn native_meta_rebuild_is_readable_by_codewalker() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let bridge_dll = std::env::var("CODEWALKER_BRIDGE_DLL")
    .unwrap_or_else(|_| "bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll".to_string());
  let codewalker = CodeWalker::init(Path::new(&bridge_dll)).expect("failed to init CodeWalker");
  let sample = sample_parent_refs_dir().join("child.ymap.xml");
  let temp_dir =
    std::env::temp_dir().join(format!("mlo_merger_native_meta_rebuild_{}", std::process::id()));
  std::fs::create_dir_all(&temp_dir).unwrap();
  let source: XmlYmap =
    quick_xml::de::from_str(&std::fs::read_to_string(&sample).unwrap()).unwrap();
  let catalog = local_ymap_schema_catalog(base);
  let bytes = xml_to_resource(
    NativeResourceFormat::Ymap,
    &std::fs::read_to_string(&sample).unwrap(),
    &catalog,
  )
  .unwrap();
  let resource = Rsc7Resource::decode(&bytes).unwrap();
  let meta = MetaResource::parse(&resource).unwrap();
  let rebuilt_resource = meta.to_rsc7(resource.version).unwrap();
  let rebuilt_binary_path = temp_dir.join("rebuilt.ymap");
  let rebuilt_xml_path = temp_dir.join("rebuilt.ymap.xml");
  std::fs::write(&rebuilt_binary_path, rebuilt_resource.encode().unwrap()).unwrap();
  codewalker.ymap_to_xml(&rebuilt_binary_path, &rebuilt_xml_path).unwrap();
  let actual: XmlYmap =
    quick_xml::de::from_str(&std::fs::read_to_string(rebuilt_xml_path).unwrap()).unwrap();
  assert_sample_entity_refs(&source, &actual, "Native META reserialization");
  std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
#[ignore]
fn native_xml_rebuild_is_readable_by_codewalker() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let bridge_dll = std::env::var("CODEWALKER_BRIDGE_DLL")
    .unwrap_or_else(|_| "bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll".to_string());
  let codewalker = CodeWalker::init(Path::new(&bridge_dll)).expect("failed to init CodeWalker");
  let input = sample_parent_refs_dir().join("child.ymap.xml");
  let temp_dir =
    std::env::temp_dir().join(format!("mlo_merger_native_xml_rebuild_{}", std::process::id()));
  std::fs::create_dir_all(&temp_dir).unwrap();
  let expected: XmlYmap =
    quick_xml::de::from_str(&std::fs::read_to_string(&input).unwrap()).unwrap();
  let catalog = local_ymap_schema_catalog(base);
  let bytes = xml_to_resource(
    NativeResourceFormat::Ymap,
    &std::fs::read_to_string(&input).unwrap(),
    &catalog,
  )
  .unwrap();
  let rebuilt_binary_path = temp_dir.join("rebuilt.ymap");
  let rebuilt_xml_path = temp_dir.join("rebuilt.ymap.xml");
  std::fs::write(&rebuilt_binary_path, bytes).unwrap();
  codewalker.ymap_to_xml(&rebuilt_binary_path, &rebuilt_xml_path).unwrap();
  let actual: XmlYmap =
    quick_xml::de::from_str(&std::fs::read_to_string(rebuilt_xml_path).unwrap()).unwrap();
  assert_sample_entity_refs(&expected, &actual, "Native XML rebuild");
  std::fs::remove_dir_all(temp_dir).unwrap();
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
  let sample = sample_parent_refs_dir().join("child.ymap.xml");
  let source: XmlYmap =
    quick_xml::de::from_str(&std::fs::read_to_string(&sample).unwrap()).unwrap();
  let tmp_dir = std::env::temp_dir()
    .join(format!("mlo_merger_native_codewalker_compare_{}", std::process::id()));
  std::fs::create_dir_all(&tmp_dir).unwrap();
  let binary_path = tmp_dir.join("sample.ymap");
  let codewalker_xml_path = tmp_dir.join("codewalker.ymap.xml");
  codewalker.xml_to_ymap(&sample, &binary_path).unwrap();
  codewalker.ymap_to_xml(&binary_path, &codewalker_xml_path).unwrap();
  let bytes = std::fs::read(binary_path).unwrap();
  let resource = Rsc7Resource::decode(&bytes).unwrap();
  let hash_names = MetaResource::parse(&resource).unwrap().hash_names();
  let native = ymap_to_xml(&bytes, &hash_names).unwrap();
  let expected: XmlYmap =
    quick_xml::de::from_str(&std::fs::read_to_string(codewalker_xml_path).unwrap()).unwrap();
  let actual: XmlYmap = quick_xml::de::from_str(&native).unwrap();
  assert_sample_entity_refs(&source, &expected, "CodeWalker sample export");
  assert_sample_entity_refs(&source, &actual, "Native sample export");
  std::fs::remove_dir_all(tmp_dir).unwrap();
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
