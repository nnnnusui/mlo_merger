use super::*;

#[test]
#[ignore = "requires locally published CodeWalker.Bridge and CodeWalker.Core.dll"]
fn codewalker_accepts_native_ybn_xml_with_vertex_quantum() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let source =
    std::fs::read_to_string(base.join("docs/sample/ybn_conflicts/geometry_bvh.ybn.xml")).unwrap();
  let native =
    xml_to_resource(NativeResourceFormat::Ybn, &source, &MetaSchemaCatalog::default()).unwrap();
  let xml =
    resource_to_xml(NativeResourceFormat::Ybn, &native, &std::collections::HashMap::new()).unwrap();
  assert!(xml.contains("<VertexQuantum "));
  let directory =
    std::env::temp_dir().join(format!("ybn_quantum_codewalker_{}", std::process::id()));
  std::fs::create_dir(&directory).unwrap();
  let input = directory.join("geometry.ybn.xml");
  let output = directory.join("geometry.ybn");
  let exported = directory.join("codewalker.ybn.xml");
  std::fs::write(&input, &xml).unwrap();
  let bridge = std::env::var("CODEWALKER_BRIDGE_DLL").unwrap_or_else(|_| {
    base
      .join("bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll")
      .to_string_lossy()
      .into_owned()
  });
  let codewalker = CodeWalker::init(Path::new(&bridge)).unwrap();
  codewalker.game_file_from_xml(&input, &output).unwrap();
  codewalker.game_file_to_xml(&output, &exported).unwrap();
  let result = std::fs::read_to_string(exported).unwrap();
  assert!(result.contains("<BoundsFile>"));
  assert!(result.contains("GeometryBVH"));
  std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn sample_ybn_conflicts_merge_vanilla_deltas_and_omit_source_files() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let samples = base.join("docs/sample/ybn_conflicts");
  let staging = std::env::temp_dir().join(format!("mlo_ybn_conflicts_{}", std::process::id()));
  let source = staging.join("source");
  let vanilla = staging.join("vanilla");
  let output = staging.join("merged_ybn");
  let omitted = staging.join("_extracted_ybns.txt");
  let resource_a = source.join("resource_a/stream");
  let resource_b = source.join("resource_b/stream/nested");
  std::fs::create_dir_all(&resource_a).unwrap();
  std::fs::create_dir_all(&resource_b).unwrap();
  std::fs::create_dir_all(&vanilla).unwrap();
  for manifest in ["resource_a", "resource_b"] {
    std::fs::write(source.join(manifest).join("fxmanifest.lua"), []).unwrap();
  }
  let baseline_xml = std::fs::read_to_string(samples.join("resource_a.ybn.xml")).unwrap();
  let changed_xml = std::fs::read_to_string(samples.join("resource_b.ybn.xml")).unwrap();
  let baseline =
    xml_to_resource(NativeResourceFormat::Ybn, &baseline_xml, &Default::default()).unwrap();
  let changed =
    xml_to_resource(NativeResourceFormat::Ybn, &changed_xml, &Default::default()).unwrap();
  std::fs::write(vanilla.join("sc1_18_0.ybn"), &baseline).unwrap();
  std::fs::write(resource_a.join("sc1_18_0.ybn"), &baseline).unwrap();
  std::fs::write(resource_b.join("sc1_18_0.ybn"), &changed).unwrap();

  MergeYbnConflicts {
    source_dir: source.clone(),
    vanilla_dir: vanilla.clone(),
    output_dir: output.clone(),
    omitted_files_path: omitted.clone(),
  }
  .run(None)
  .unwrap();

  let merged_path = output.join("sc1_18_0.ybn");
  let merged_xml =
    mlo_merger::core::format::ybn::ybn_to_xml(&std::fs::read(&merged_path).unwrap()).unwrap();
  assert_eq!(merged_xml.matches("<Item type=\"Box\">").count(), 1);
  assert!(merged_xml.contains("<BoxMin x=\"9\" y=\"-1\" z=\"-1\" />"));
  assert!(merged_xml.contains("<BoundsFile>"));
  let omitted_paths = std::fs::read_to_string(omitted).unwrap();
  assert!(omitted_paths.contains("resource_a/stream/sc1_18_0.ybn"));
  assert!(omitted_paths.contains("resource_b/stream/nested/sc1_18_0.ybn"));
  std::fs::remove_dir_all(staging).unwrap();
}

#[test]
#[ignore = "requires local source/vanilla YBN corpora and CodeWalker.Core.dll"]
fn all_local_ybn_conflicts_merge_against_vanilla_and_reopen_with_codewalker() {
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let bridge = std::env::var("CODEWALKER_BRIDGE_DLL").unwrap_or_else(|_| {
    base.join("bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll").display().to_string()
  });
  let codewalker = CodeWalker::init(Path::new(&bridge)).unwrap();
  let staging = std::env::temp_dir().join(format!("mlo_ybn_corpus_merge_{}", std::process::id()));
  let output = staging.join("merged_ybn");
  let omitted = staging.join("_extracted_ybns.txt");
  MergeYbnConflicts {
    source_dir: base.join("asset/source"),
    vanilla_dir: base.join("asset/vanilla/ybn"),
    output_dir: output.clone(),
    omitted_files_path: omitted.clone(),
  }
  .run(Some(&codewalker))
  .unwrap();

  let binaries = std::fs::read_dir(&output)
    .unwrap()
    .map(|entry| entry.unwrap().path())
    .filter(|path| path.extension().is_some_and(|extension| extension == "ybn"))
    .collect::<Vec<_>>();
  assert!(!binaries.is_empty(), "no YBN conflicts were merged");
  for binary in &binaries {
    let xml = staging.join(format!("{}.xml", binary.file_name().unwrap().to_string_lossy()));
    codewalker.game_file_to_xml(binary, &xml).unwrap();
    assert!(std::fs::read_to_string(xml).unwrap().contains("<BoundsFile>"));
  }
  let omitted_lines = std::fs::read_to_string(omitted).unwrap().lines().count();
  assert!(omitted_lines >= binaries.len() * 2, "all colliding source YBNs must be omitted");
  eprintln!("Merged and reopened {} vanilla-relative YBN conflicts", binaries.len());
  std::fs::remove_dir_all(staging).unwrap();
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
