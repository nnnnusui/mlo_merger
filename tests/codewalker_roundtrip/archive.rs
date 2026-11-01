use super::*;

#[test]
#[ignore = "requires a successfully generated asset/vanilla-cache schema 2 or 3 cache"]
fn gtav_generated_version_cache_has_complete_artifacts_without_log_files() {
  use mlo_merger::core::vanilla_cache::VanillaCacheManifest;

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
    assert!(!directory.join("create_cache.log").exists());
    for (name, change) in &version.changes {
      let artifact = root.join(&change.file.object);
      assert_eq!(change.file.sha256.len(), 64);
      let family = if name.ends_with(".ybn") { "ybn" } else { "ymap" };
      assert!(change.file.object.starts_with(&format!("{}/{family}/", version.id)));
      if change.previous_sha256.is_some() {
        modified += 1;
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
          // Schema 2 reports hold the removed merge diff; only their JSON form is checked.
          let _: serde_json::Value = read_json(&artifact);
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
