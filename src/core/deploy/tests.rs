use super::*;

use crate::core::source_cache::write_test_inventory;

#[test]
fn deploy_copies_only_changed_files_and_removes_only_managed_stale_files() {
  let root = std::env::temp_dir().join(format!("deploy_incremental_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let merged = root.join("merged");
  let cache = root.join("cache");
  let output = root.join("merged_mlo");
  fs::create_dir_all(merged.join("ymap")).unwrap();
  fs::create_dir_all(cache.join("resources/resource_a/nested")).unwrap();
  fs::create_dir_all(cache.join("_old/timestamp/resource_a")).unwrap();
  fs::create_dir_all(output.join("stream/obsolete/clone/resource/nested")).unwrap();
  fs::create_dir_all(output.join("stream/unmanaged")).unwrap();
  fs::write(output.join("stream/unmanaged/keep.txt"), b"keep").unwrap();
  fs::write(merged.join("ymap/map.ymap"), b"merged map").unwrap();
  fs::write(merged.join("_omit.txt"), b"source paths").unwrap();
  fs::write(cache.join("resources/resource_a/nested/MAP.YMAP"), b"must not clone").unwrap();
  fs::write(cache.join("resources/resource_a/nested/collision.ybn"), b"clone bytes").unwrap();
  fs::write(cache.join("_old/timestamp/resource_a/archived.ybn"), b"archive").unwrap();
  // Not recorded by the inventory, so not part of the cache.
  fs::write(cache.join("resources/resource_a/nested/stray.ybn"), b"stray").unwrap();
  let inventory = |cached: &[&str]| write_test_inventory(&cache, cached);
  inventory(&["resources/resource_a/nested/MAP.YMAP", "resources/resource_a/nested/collision.ybn"]);
  let deploy = Deploy {
    merged_dir: merged.clone(),
    source_cache_dir: cache.clone(),
    output_dir: output.clone(),
    force: false,
  };
  assert_eq!(
    deploy.run().unwrap(),
    DeploySummary {
      copied: 2,
      skipped: 0,
      removed: 0
    }
  );
  let cloned = output.join("stream/ybn/clone/resource_a/nested/collision.ybn");
  assert!(!output.join("stream/obsolete").exists());
  assert!(!output.join("stream/ybn/clone/resource_a/nested/stray.ybn").exists());
  assert!(output.join("stream/unmanaged/keep.txt").is_file());
  assert_eq!(
    fs::read_to_string(output.join("files.txt")).unwrap(),
    "resource_a/stream/nested/MAP.YMAP\nresource_a/stream/nested/collision.ybn"
  );
  let manifest = output.join("fxmanifest.lua");
  assert_eq!(
    fs::read_to_string(&manifest).unwrap(),
    "fx_version 'cerulean'\ngame 'gta5'\nversion '1.0.0'\nthis_is_a_map 'yes'\n\ndependency '/assetpacks'\n\n"
  );
  fs::write(&manifest, b"custom manifest").unwrap();
  let manifest_stamp = fs::metadata(&manifest).unwrap().modified().unwrap();
  assert_eq!(fs::read(output.join("stream/ymap/merged/map.ymap")).unwrap(), b"merged map");
  assert_eq!(fs::read(&cloned).unwrap(), b"clone bytes");
  assert!(!output.join("stream/ymap/clone").exists());
  assert!(!output.join("_omit.txt").exists());
  let stamp = fs::metadata(&cloned).unwrap().modified().unwrap();
  assert_eq!(
    stamp,
    fs::metadata(cache.join("resources/resource_a/nested/collision.ybn"))
      .unwrap()
      .modified()
      .unwrap()
  );
  assert_eq!(
    deploy.run().unwrap(),
    DeploySummary {
      copied: 0,
      skipped: 2,
      removed: 0
    }
  );
  assert_eq!(fs::metadata(&cloned).unwrap().modified().unwrap(), stamp);
  assert_eq!(fs::read(&manifest).unwrap(), b"custom manifest");
  assert_eq!(fs::metadata(&manifest).unwrap().modified().unwrap(), manifest_stamp);
  fs::File::options()
    .write(true)
    .open(cache.join("resources/resource_a/nested/collision.ybn"))
    .unwrap()
    .set_times(
      fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(10)),
    )
    .unwrap();
  assert_eq!(deploy.run().unwrap().copied, 0);
  fs::write(&cloned, b"damaged").unwrap();
  assert_eq!(deploy.run().unwrap().copied, 1);
  assert_eq!(fs::read(&cloned).unwrap(), b"clone bytes");
  fs::write(cache.join("resources/resource_a/nested/collision.ybn"), b"changed clone content")
    .unwrap();
  assert_eq!(deploy.run().unwrap().copied, 1);
  assert_eq!(fs::read(&cloned).unwrap(), b"changed clone content");
  fs::remove_file(output.join("stream/ymap/merged/map.ymap")).unwrap();
  assert_eq!(deploy.run().unwrap().copied, 1);
  fs::write(merged.join("ymap/map.ymap"), b"changed merged map").unwrap();
  assert_eq!(deploy.run().unwrap().copied, 1);
  fs::write(output.join("unmanaged.txt"), b"keep").unwrap();
  fs::create_dir_all(merged.join("ybn")).unwrap();
  fs::write(merged.join("ybn/collision.ybn"), b"new merged collision").unwrap();
  assert_eq!(
    deploy.run().unwrap(),
    DeploySummary {
      copied: 1,
      skipped: 1,
      removed: 1
    }
  );
  assert!(!cloned.exists());
  assert!(!output.join("stream/ybn/clone").exists());
  assert!(output.join("stream/ybn/merged/collision.ybn").is_file());
  assert!(output.join("unmanaged.txt").exists());
  fs::remove_file(merged.join("ybn/collision.ybn")).unwrap();
  assert_eq!(
    deploy.run().unwrap(),
    DeploySummary {
      copied: 1,
      skipped: 1,
      removed: 1
    }
  );
  assert!(cloned.is_file());
  assert!(!output.join("stream/ybn/merged").exists());
  let mut forced = deploy.clone();
  forced.force = true;
  assert_eq!(forced.run().unwrap().copied, 2);
  let metadata: DeployMetadata =
    serde_json::from_reader(fs::File::open(output.join("deploy_cache_info.json")).unwrap())
      .unwrap();
  assert_eq!(metadata.files.len(), 2);
  assert!(metadata.files.values().all(|file| file.input.sha256 == file.output.sha256));
  fs::write(cache.join("resources/resource_a/nested/model.ytyp"), b"cached model").unwrap();
  assert_eq!(deploy.run().unwrap().copied, 0, "an unrecorded file is not deployed");
  inventory(&[
    "resources/resource_a/nested/MAP.YMAP",
    "resources/resource_a/nested/collision.ybn",
    "resources/resource_a/nested/model.ytyp",
  ]);
  assert_eq!(deploy.run().unwrap().copied, 1);
  assert_eq!(
    fs::read(output.join("stream/ytyp/clone/resource_a/nested/model.ytyp")).unwrap(),
    b"cached model"
  );
  fs::write(&cloned, b"user edit").unwrap();
  fs::remove_file(cache.join("resources/resource_a/nested/collision.ybn")).unwrap();
  assert!(deploy.run().unwrap_err().to_string().contains("Recorded cached file is missing"));
  inventory(&["resources/resource_a/nested/MAP.YMAP", "resources/resource_a/nested/model.ytyp"]);
  assert!(deploy.run().unwrap_err().to_string().contains("Refusing to remove modified"));
  assert_eq!(fs::read(&cloned).unwrap(), b"user edit");
  let mut overlapping = deploy.clone();
  overlapping.output_dir = merged;
  assert!(overlapping.run().is_err());
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn deploy_directory_cleanup_keeps_the_root_and_does_not_follow_links() {
  let root = std::env::temp_dir().join(format!("deploy_empty_directories_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let output = root.join("merged_mlo");
  fs::create_dir_all(output.join("stream/ymap/clone/resource/nested")).unwrap();
  fs::write(output.join("deploy_cache_info.json"), b"metadata").unwrap();
  fs::write(output.join("files.txt"), b"source paths").unwrap();
  remove_empty_stream_directories(&output).unwrap();
  assert!(!output.join("stream").exists());
  assert!(output.is_dir());
  assert!(output.join("deploy_cache_info.json").is_file());
  assert!(output.join("files.txt").is_file());
  remove_empty_stream_directories(&output).unwrap();
  #[cfg(unix)]
  {
    let external = root.join("external");
    fs::create_dir_all(external.join("empty/nested")).unwrap();
    fs::create_dir_all(output.join("stream/clone")).unwrap();
    std::os::unix::fs::symlink(&external, output.join("stream/clone/link")).unwrap();
    remove_empty_stream_directories(&output).unwrap();
    assert!(external.join("empty/nested").is_dir());
    assert!(
      fs::symlink_metadata(output.join("stream/clone/link")).unwrap().file_type().is_symlink()
    );
    fs::remove_dir_all(output.join("stream")).unwrap();
    std::os::unix::fs::symlink(&external, output.join("stream")).unwrap();
    assert!(remove_empty_stream_directories(&output).is_err());
    assert!(external.join("empty/nested").is_dir());
  }
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn deploy_rejects_duplicate_merged_names_and_unsafe_cache_entries() {
  let root = std::env::temp_dir().join(format!("deploy_paths_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let merged = root.join("merged");
  let cache = root.join("cache");
  let output = root.join("output");
  fs::create_dir_all(merged.join("ymap")).unwrap();
  write_test_inventory(&cache, &[]);
  fs::write(merged.join("ymap/map.ymap"), b"map").unwrap();
  fs::write(merged.join("MAP.YMAP"), b"duplicate").unwrap();
  let deploy = Deploy {
    merged_dir: merged.clone(),
    source_cache_dir: cache,
    output_dir: output.clone(),
    force: false,
  };
  assert!(deploy.run().unwrap_err().to_string().contains("Duplicate merged stream basename"));
  assert!(!output.join("stream").exists());
  fs::remove_file(merged.join("MAP.YMAP")).unwrap();
  assert_eq!(deploy.run().unwrap().copied, 1);
  let mut metadata: DeployMetadata =
    serde_json::from_reader(fs::File::open(output.join("deploy_cache_info.json")).unwrap())
      .unwrap();
  let record = metadata.files.remove("stream/ymap/merged/map.ymap").unwrap();
  metadata.files.insert("../outside.ymap".into(), record);
  write_json(&output.join("deploy_cache_info.json"), &metadata).unwrap();
  assert!(
    deploy
      .run()
      .unwrap_err()
      .to_string()
      .contains("Managed deployment files must be under stream/")
  );
  assert_eq!(fs::read(output.join("stream/ymap/merged/map.ymap")).unwrap(), b"map");
  #[cfg(unix)]
  {
    fs::remove_file(output.join("deploy_cache_info.json")).unwrap();
    std::os::unix::fs::symlink(&merged, output.join("linked")).unwrap();
    let mut linked = deploy.clone();
    linked.output_dir = output.join("linked");
    assert!(linked.run().is_err());
  }
  fs::remove_dir_all(root).unwrap();
}
