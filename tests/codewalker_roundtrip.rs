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
};
use mlo_merger::core::format::ymap::xml::XmlYmap;
use mlo_merger::core::merge::{run::MergeYmapXml, ybn_conflicts::MergeYbnConflicts};
use mlo_merger::core::xmlconvert::Xml2Ymap;

fn sample_parent_refs_dir() -> std::path::PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/parent_refs")
}

#[test]
#[ignore = "requires a successfully generated asset/gtav-cache schema 2 cache"]
fn gtav_generated_version_cache_has_complete_artifacts_and_diff_logs() {
  use mlo_merger::core::{gtav_cache::GtavCacheManifest, merge::YmapDiff};
  use std::io::BufRead;

  fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(path).unwrap())).unwrap()
  }

  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/gtav-cache");
  let manifest: GtavCacheManifest = read_json(&root.join("cache_info.json"));
  assert_eq!(manifest.format_version, 2);
  assert!(manifest.versions[0].archives.contains(&"common.rpf".into()));
  assert!(manifest.versions[0].archives.contains(&"x64a.rpf".into()));
  let mut added = 0;
  let mut modified = 0;
  for version in &manifest.versions {
    let directory = root.join(&version.id);
    assert!(directory.join("ymap").is_dir());
    let metadata: serde_json::Value = read_json(&directory.join("version_info.json"));
    assert_eq!(metadata["id"], version.id);
    assert_eq!(metadata["changes"].as_object().unwrap().len(), version.changes.len());
    let log = directory.join("create_cache.log");
    assert!(std::fs::metadata(&log).unwrap().len() > 0);
    let has_diff = std::io::BufReader::new(std::fs::File::open(log).unwrap())
      .lines()
      .any(|line| line.unwrap().contains("Diff "));
    for (name, change) in &version.changes {
      let artifact = root.join(&change.file.object);
      assert_eq!(change.file.sha256.len(), 64);
      assert!(change.file.object.starts_with(&format!("{}/ymap/", version.id)));
      if change.previous_sha256.is_some() {
        modified += 1;
        assert!(has_diff, "missing diff log in {}", version.id);
        assert_eq!(artifact.file_name().unwrap().to_string_lossy(), format!("{name}.diff.json"));
        let _: YmapDiff = read_json(&artifact);
      } else {
        added += 1;
        assert_eq!(artifact.file_name().unwrap().to_string_lossy(), *name);
        assert!(std::fs::metadata(artifact).unwrap().len() > 0);
      }
    }
  }
  let latest = manifest.resolve_version(&manifest.versions.last().unwrap().id).unwrap();
  assert_eq!(latest.len(), added);
  assert!(modified > 0);
  println!(
    "Validated {} versions: {added} additions, {modified} diff reports",
    manifest.versions.len()
  );
}

#[test]
#[ignore = "requires locally published CodeWalker.Bridge and CodeWalker.Core.dll"]
fn gtav_rpf_extracts_nested_resources_and_preserves_headers() {
  use std::io::{Read, Write};

  fn fixture(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut names = vec![0];
    let mut offsets = vec![];
    for (name, _, _) in entries {
      offsets.push(names.len() as u64);
      names.extend_from_slice(name.as_bytes());
      names.push(0);
    }
    let mut bytes = vec![];
    for value in [0x52504637u32, entries.len() as u32 + 1, names.len() as u32, 0x4e45504f] {
      bytes.extend_from_slice(&value.to_le_bytes());
    }
    for value in [0u32, 0x7fffff00, 1, entries.len() as u32] {
      bytes.extend_from_slice(&value.to_le_bytes());
    }
    let mut block = (16 + (entries.len() + 1) * 16 + names.len()).div_ceil(512);
    let mut blocks = vec![];
    for ((_, data, resource), name_offset) in entries.iter().zip(offsets) {
      blocks.push(block);
      let size = if *resource { data.len() as u64 } else { 0 };
      let file_offset = block as u64 | if *resource { 0x800000 } else { 0 };
      bytes.extend_from_slice(&(name_offset | (size << 16) | (file_offset << 40)).to_le_bytes());
      bytes.extend_from_slice(
        &(if *resource { 0x08000000u32 } else { data.len() as u32 }).to_le_bytes(),
      );
      bytes.extend_from_slice(&0u32.to_le_bytes());
      block += data.len().div_ceil(512);
    }
    bytes.extend_from_slice(&names);
    for ((_, data, _), block) in entries.iter().zip(blocks) {
      bytes.resize(block * 512, 0);
      bytes.extend_from_slice(data);
    }
    bytes.resize(bytes.len().div_ceil(512) * 512, 0);
    bytes
  }

  let payload = vec![42u8; 512];
  let mut encoder = flate2::write::DeflateEncoder::new(vec![], flate2::Compression::default());
  encoder.write_all(&payload).unwrap();
  let mut resource = b"RSC7".to_vec();
  for value in [0u32, 0x08000000, 0] {
    resource.extend_from_slice(&value.to_le_bytes());
  }
  resource.extend(encoder.finish().unwrap());
  let nested = fixture(&[("inside.ymap", &resource, true), ("ignored.txt", b"skip", false)]);
  let archive = fixture(&[("outside.ymap", b"outer", false), ("nested.rpf", &nested, false)]);
  let directory = std::env::temp_dir().join(format!("gtav_rpf_fixture_{}", std::process::id()));
  std::fs::create_dir_all(&directory).unwrap();
  let input = directory.join("fixture.rpf");
  std::fs::write(&input, archive).unwrap();
  let bridge = std::env::var("CODEWALKER_BRIDGE_DLL").unwrap_or_else(|_| {
    format!(
      "{}/bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll",
      env!("CARGO_MANIFEST_DIR")
    )
  });
  let codewalker = CodeWalker::init(Path::new(&bridge)).unwrap();
  let paths = directory.join("archives.json");
  codewalker.list_rpf_paths(&input, &paths).unwrap();
  let paths: Vec<String> = serde_json::from_slice(&std::fs::read(paths).unwrap()).unwrap();
  assert_eq!(paths, ["fixture.rpf", "fixture.rpf/nested.rpf"]);
  let all = directory.join("all");
  codewalker.extract_rpf(&input, &all).unwrap();
  let files: Vec<serde_json::Value> =
    serde_json::from_slice(&std::fs::read(all.join("files.json")).unwrap()).unwrap();
  assert_eq!(files.len(), 2);
  let selected = directory.join("selected");
  codewalker.extract_rpf_subtree(&input, "fixture.rpf/nested.rpf", &selected).unwrap();
  let files: Vec<serde_json::Value> =
    serde_json::from_slice(&std::fs::read(selected.join("files.json")).unwrap()).unwrap();
  assert_eq!(files.len(), 1);
  assert_eq!(files[0]["source"], "fixture.rpf/nested.rpf/inside.ymap");
  assert_eq!(files[0]["sha256"].as_str().unwrap().len(), 64);
  let extracted = std::fs::read(selected.join(files[0]["stored"].as_str().unwrap())).unwrap();
  assert_eq!(&extracted[..16], &resource[..16]);
  let mut decoded = vec![];
  flate2::read::DeflateDecoder::new(&extracted[16..]).read_to_end(&mut decoded).unwrap();
  assert_eq!(decoded, payload);
  std::fs::write(directory.join("invalid.rpf"), b"invalid archive").unwrap();
  assert!(
    codewalker.extract_rpf(&directory.join("invalid.rpf"), &directory.join("invalid")).is_err()
  );
  std::fs::remove_dir_all(directory).unwrap();
}

fn local_ymap_schema_catalog(base: &Path) -> MetaSchemaCatalog {
  let mut catalog = MetaSchemaCatalog::default();
  for entry in walkdir::WalkDir::new(base.join("asset/extracted")) {
    let Ok(entry) = entry else {
      continue;
    };
    if !entry.file_type().is_file()
      || entry.path().extension().is_none_or(|extension| extension != "ymap")
    {
      continue;
    }
    let Ok(bytes) = std::fs::read(entry.path()) else {
      continue;
    };
    let Ok(resource) = Rsc7Resource::decode(&bytes) else {
      continue;
    };
    if let Ok(meta) = MetaResource::parse(&resource) {
      catalog.add_resource(&meta);
    }
  }
  catalog
}

fn assert_sample_entity_refs(
  expected: &XmlYmap,
  actual: &XmlYmap,
  label: &str,
) {
  assert_eq!(actual.entities.items.len(), expected.entities.items.len(), "entity count: {label}");
  for (expected, actual) in expected.entities.items.iter().zip(&actual.entities.items) {
    assert_eq!(actual.guid.value, expected.guid.value, "entity GUID: {label}");
    assert_eq!(actual.parent_index.value, expected.parent_index.value, "parentIndex: {label}");
    assert_eq!(actual.flags.value & 8, expected.flags.value & 8, "external-parent flag: {label}");
  }
}

#[test]
#[ignore = "requires local YMAP schemas and CodeWalker.Core.dll"]
fn sample_parent_relink_survives_native_and_codewalker_rebuild() {
  use mlo_merger::core::format::ymap::xml::XmlYmap;
  let base = Path::new(env!("CARGO_MANIFEST_DIR"));
  let staging = std::env::temp_dir().join(format!("mlo_parent_refs_binary_{}", std::process::id()));
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
  MergeYmapXml {
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
  let mut catalog = MetaSchemaCatalog::default();
  for entry in walkdir::WalkDir::new(base.join("asset/extracted")) {
    let Ok(entry) = entry else {
      continue;
    };
    if !entry.file_type().is_file()
      || entry.path().extension().is_none_or(|extension| extension != "ymap")
    {
      continue;
    }
    let Ok(bytes) = std::fs::read(entry.path()) else {
      continue;
    };
    let Ok(resource) = Rsc7Resource::decode(&bytes) else {
      continue;
    };
    if let Ok(meta) = MetaResource::parse(&resource) {
      catalog.add_resource(&meta);
    }
  }
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
    mlo_merger::core::format::gamefile::ybn::ybn_to_xml(&std::fs::read(&merged_path).unwrap())
      .unwrap();
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
  if failures.is_empty() {
    std::fs::remove_dir_all(&temp_dir).unwrap();
  } else {
    eprintln!("Preserving CodeWalker comparison artifacts under {}", temp_dir.display());
  }
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
