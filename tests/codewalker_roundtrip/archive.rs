use super::*;

#[test]
#[ignore = "requires a successfully generated asset/vanilla-cache schema 2 or 3 cache"]
fn gtav_generated_version_cache_has_complete_artifacts_and_diff_logs() {
  use mlo_merger::core::{merge::YmapDiff, vanilla_cache::VanillaCacheManifest};
  use std::io::BufRead;

  fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    serde_json::from_reader(std::io::BufReader::new(std::fs::File::open(path).unwrap())).unwrap()
  }

  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache");
  let manifest: VanillaCacheManifest = read_json(&root.join("cache_info.json"));
  assert!(matches!(manifest.format_version, 2 | 3));
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
      let family = if name.ends_with(".ybn") { "ybn" } else { "ymap" };
      assert!(change.file.object.starts_with(&format!("{}/{family}/", version.id)));
      if change.previous_sha256.is_some() {
        modified += 1;
        assert!(has_diff, "missing diff log in {}", version.id);
        assert_eq!(artifact.file_name().unwrap().to_string_lossy(), format!("{name}.diff.json"));
        if manifest.format_version == 3 {
          let delta: serde_json::Value = read_json(&artifact);
          assert_eq!(
            delta["format"],
            if family == "ybn" { "vanilla_ybn_delta_v1" } else { "vanilla_ymap_delta_v1" }
          );
          assert_eq!(delta["target_native_sha256"], change.file.sha256);
          if family == "ybn" {
            assert!(delta["replacement"].is_array());
            assert!(delta["prefix_length"].is_number());
          } else {
            assert!(delta["changes"].is_array());
            assert!(delta["entity_order"].is_array());
          }
        } else {
          let _: YmapDiff = read_json(&artifact);
        }
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
  let nested = fixture(&[
    ("inside.ymap", &resource, true),
    ("collision.ybn", &resource, true),
    ("ignored.txt", b"skip", false),
  ]);
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
  assert_eq!(files.len(), 3);
  let selected = directory.join("selected");
  codewalker.extract_rpf_subtree(&input, "fixture.rpf/nested.rpf", &selected).unwrap();
  let files: Vec<serde_json::Value> =
    serde_json::from_slice(&std::fs::read(selected.join("files.json")).unwrap()).unwrap();
  assert_eq!(files.len(), 2);
  assert_eq!(files[0]["source"], "fixture.rpf/nested.rpf/inside.ymap");
  assert_eq!(files[0]["sha256"].as_str().unwrap().len(), 64);
  let extracted = std::fs::read(selected.join(files[0]["stored"].as_str().unwrap())).unwrap();
  assert_eq!(&extracted[..16], &resource[..16]);
  let mut decoded = vec![];
  flate2::read::DeflateDecoder::new(&extracted[16..]).read_to_end(&mut decoded).unwrap();
  assert_eq!(decoded, payload);
  let bounds = files.iter().find(|file| file["name"] == "collision.ybn").unwrap();
  assert_eq!(bounds["source"], "fixture.rpf/nested.rpf/collision.ybn");
  let extracted = std::fs::read(selected.join(bounds["stored"].as_str().unwrap())).unwrap();
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
