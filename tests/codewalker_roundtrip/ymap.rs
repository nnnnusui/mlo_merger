use super::*;

#[test]
#[ignore = "requires CodeWalker.Core.dll"]
fn sample_parent_relink_survives_native_and_codewalker_rebuild() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let staging = std::env::temp_dir().join(format!("mlo_parent_refs_binary_{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&staging);
  let samples = sample_parent_refs_dir();
  let output = staging.join("output");
  let reports = staging.join("reports");
  for directory in ["vanilla", "a", "b", "reports"] {
    std::fs::create_dir_all(staging.join(directory)).unwrap();
  }
  let vanilla = [
    ("vanilla_parent.ymap", "parent.ymap"),
    ("child.ymap", "child.ymap"),
    ("child.ymap", "dependent.ymap"),
  ]
  .map(|(sample, name)| {
    let path = staging.join("vanilla").join(name);
    std::fs::copy(samples.join(sample), &path).unwrap();
    path
  });
  let sources = [
    ("resource_a", "resource_a_parent.ymap", "parent.ymap"),
    ("resource_b", "resource_b_parent.ymap", "parent.ymap"),
    ("resource_a", "child.ymap", "child.ymap"),
  ]
  .map(|(resource, sample, name)| {
    let path = staging.join(&resource[resource.len() - 1..]).join(name);
    std::fs::copy(samples.join(sample), &path).unwrap();
    (resource.to_string(), path, None)
  });
  MergeYmap {
    vanilla_dir: staging.join("vanilla"),
    output_dir: output.clone(),
    rebuild_all: false,
    blacklist_config: None,
    overwrites: Default::default(),
    vanilla_overwrites: Default::default(),
  }
  .run_with_latest_vanilla_files(&vanilla, &sources)
  .unwrap();
  let bridge = std::env::var("CODEWALKER_BRIDGE_DLL").unwrap_or_else(|_| {
    base.join("bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll").display().to_string()
  });
  let codewalker = CodeWalker::init(Path::new(&bridge)).unwrap();
  let mut native_maps = Vec::new();
  let mut dll_maps = Vec::new();
  for name in ["parent", "child", "dependent"] {
    let native_path = output.join(format!("{name}.ymap"));
    let native_map = read_ymap(&native_path);
    let dll_map = codewalker_rebuild(&codewalker, &native_path, &reports, name);
    let expected = native_map.entities.iter().map(|entity| entity.guid).collect::<Vec<_>>();
    assert_eq!(
      dll_map.entities.iter().map(|entity| entity.guid).collect::<Vec<_>>(),
      expected,
      "entity order differs after rebuilding {name}"
    );
    native_maps.push(native_map);
    dll_maps.push(dll_map);
  }
  for maps in [&native_maps, &dll_maps] {
    assert_eq!(maps[0].entities.len(), 1);
    assert_eq!(maps[0].entities[0].guid, 200);
    for child in [&maps[1].entities[0], &maps[2].entities[0]] {
      assert_eq!(child.parent_index, 0);
      assert_eq!(maps[0].entities[child.parent_index as usize].guid, 200);
      assert_eq!(child.flags & 8, 8);
    }
  }
  eprintln!(
    "Native/DLL sample children resolve parent index 0 to GUID 200; entity order is retained"
  );
  std::fs::remove_dir_all(staging).unwrap();
}

#[test]
#[ignore]
fn ymap_binary_round_trip() {
  let bridge_dll = std::env::var("CODEWALKER_BRIDGE_DLL")
    .unwrap_or_else(|_| "bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll".to_string());
  let codewalker = CodeWalker::init(Path::new(&bridge_dll)).expect("failed to init CodeWalker");
  let sample = sample_parent_refs_dir().join("child.ymap");
  let tmp_dir =
    std::env::temp_dir().join(format!("mlo_merger_codewalker_roundtrip_{}", std::process::id()));
  std::fs::create_dir_all(&tmp_dir).unwrap();
  let first = codewalker_rebuild(&codewalker, &sample, &tmp_dir, "first");
  let second = codewalker_rebuild(&codewalker, &tmp_dir.join("first.dll.ymap"), &tmp_dir, "second");
  assert_sample_entity_refs(&read_ymap(&sample), &first, "CodeWalker XML/binary roundtrip");
  assert_sample_entity_refs(&first, &second, "CodeWalker XML/binary roundtrip");
  std::fs::remove_dir_all(tmp_dir).unwrap();
}

#[test]
#[ignore]
fn native_meta_rebuild_is_readable_by_codewalker() {
  let bridge_dll = std::env::var("CODEWALKER_BRIDGE_DLL")
    .unwrap_or_else(|_| "bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll".to_string());
  let codewalker = CodeWalker::init(Path::new(&bridge_dll)).expect("failed to init CodeWalker");
  let sample = sample_parent_refs_dir().join("child.ymap");
  let temp_dir =
    std::env::temp_dir().join(format!("mlo_merger_native_meta_rebuild_{}", std::process::id()));
  std::fs::create_dir_all(&temp_dir).unwrap();
  let resource = Rsc7Resource::decode(&std::fs::read(&sample).unwrap()).unwrap();
  let meta = MetaResource::parse(&resource).unwrap();
  let rebuilt_binary_path = temp_dir.join("rebuilt.ymap");
  std::fs::write(&rebuilt_binary_path, meta.to_rsc7(resource.version).unwrap().encode().unwrap())
    .unwrap();
  let actual = codewalker_rebuild(&codewalker, &rebuilt_binary_path, &temp_dir, "rebuilt");
  assert_sample_entity_refs(&read_ymap(&sample), &actual, "Native META reserialization");
  std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
#[ignore]
fn native_xml_rebuild_is_readable_by_codewalker() {
  let bridge_dll = std::env::var("CODEWALKER_BRIDGE_DLL")
    .unwrap_or_else(|_| "bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll".to_string());
  let codewalker = CodeWalker::init(Path::new(&bridge_dll)).expect("failed to init CodeWalker");
  let sample = sample_parent_refs_dir().join("child.ymap");
  let temp_dir =
    std::env::temp_dir().join(format!("mlo_merger_native_xml_rebuild_{}", std::process::id()));
  std::fs::create_dir_all(&temp_dir).unwrap();
  let bytes = std::fs::read(&sample).unwrap();
  let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
  let mut catalog = MetaSchemaCatalog::default();
  catalog.add_resource(&meta);
  let xml = StreamFile::decode_to_xml(FileType::Ymap, &bytes, &meta.hash_names()).unwrap();
  let rebuilt_binary_path = temp_dir.join("rebuilt.ymap");
  std::fs::write(
    &rebuilt_binary_path,
    StreamFile::encode_xml(FileType::Ymap, &xml, &catalog).unwrap(),
  )
  .unwrap();
  let actual = codewalker_rebuild(&codewalker, &rebuilt_binary_path, &temp_dir, "rebuilt");
  assert_sample_entity_refs(&read_ymap(&sample), &actual, "Native XML rebuild");
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

  let (_, failed) =
    mlo_merger::core::conversion::from_xml(&input_dir, &output_dir, Some(&schema_dir)).unwrap();
  assert_eq!(failed, 0);

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

  let (_, failed) = mlo_merger::core::conversion::from_xml(
    &input_dir,
    &output_dir.join("native-ymap"),
    Some(&schema_dir),
  )
  .unwrap();
  assert_eq!(failed, 0);

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
  let sample = sample_parent_refs_dir().join("child.ymap");
  let tmp_dir = std::env::temp_dir()
    .join(format!("mlo_merger_native_codewalker_compare_{}", std::process::id()));
  std::fs::create_dir_all(&tmp_dir).unwrap();
  let codewalker_xml_path = tmp_dir.join("codewalker.ymap.xml");
  codewalker.ymap_to_xml(&sample, &codewalker_xml_path).unwrap();
  let bytes = std::fs::read(&sample).unwrap();
  let hash_names =
    MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap().hash_names();
  let native = StreamFile::decode_to_xml(FileType::Ymap, &bytes, &hash_names).unwrap();
  let expected = std::fs::read_to_string(codewalker_xml_path).unwrap();
  assert_canonical_xml_eq(&native, &expected, "Native sample export").unwrap();
  std::fs::remove_dir_all(tmp_dir).unwrap();
}

#[test]
#[ignore]
fn native_ymap_xml_matches_codewalker_corpus() {
  let bridge_dll = std::env::var("CODEWALKER_BRIDGE_DLL")
    .unwrap_or_else(|_| "bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll".to_string());
  let codewalker = CodeWalker::init(Path::new(&bridge_dll)).expect("failed to init CodeWalker");
  let input_dir = Path::new("asset/source-cache/resources");
  let tmp_dir = std::env::temp_dir().join("mlo_merger_native_codewalker_corpus");
  std::fs::create_dir_all(&tmp_dir).unwrap();
  let inputs = walkdir::WalkDir::new(input_dir)
    .sort_by_file_name()
    .into_iter()
    .map(|entry| entry.unwrap().into_path())
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
    if let Ok(meta) =
      Rsc7Resource::decode(&bytes).and_then(|resource| MetaResource::parse(&resource))
    {
      shared_names.extend(meta.hash_names());
    }

    // Resources reuse vanilla file names, so flatten the relative path to keep outputs apart.
    let relative = input.strip_prefix(input_dir).unwrap().to_string_lossy().replace('/', "__");
    let output = tmp_dir.join(format!("{relative}.xml"));
    codewalker.ymap_to_xml(input, &output).unwrap();
    references.push((input.clone(), output));
  }

  let mut compared = 0usize;
  let mut codewalker_errors = 0usize;
  for (input, reference_path) in &references {
    let bytes = std::fs::read(input).unwrap();
    let native = StreamFile::decode_to_xml(FileType::Ymap, &bytes, &shared_names)
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
