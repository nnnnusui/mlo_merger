//! GTAV archive, stage, logging and publication regression tests.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use super::{
  archives::{
    DlcList, ExtractedFile, base_archives, dlc_archive, patch_dlc, platform_virtual_path,
  },
  publication::{Staging, preserve_failed_logs, publish_cache, reset_output_directory},
  stage::stage,
};

#[test]
fn dlclist_keeps_order_and_normalizes_mounts() {
  let list: DlcList = quick_xml::de::from_str(
    "<SMandatoryPacksData><Paths><Item>dlcpacks:\\MPBeach\\</Item><Unused/><Item>platform:/dlcpacks/mpbusiness/</Item></Paths></SMandatoryPacksData>",
  ).unwrap();
  assert_eq!(
    dlc_archive(&list.paths.items[0]).unwrap(),
    PathBuf::from("update/x64/dlcpacks/mpbeach/dlc.rpf")
  );
  assert_eq!(
    dlc_archive(&list.paths.items[1]).unwrap(),
    PathBuf::from("x64/dlcpacks/mpbusiness/dlc.rpf")
  );
  assert!(dlc_archive("dlcpacks:/../escape/").is_err());
  assert!(dlc_archive("unknown:/mpbeach/").is_err());
}

#[test]
fn resets_only_cache_output_and_refuses_game_or_workspace_paths() {
  let root = std::env::temp_dir().join(format!("gtav_reset_output_{}", std::process::id()));
  let game = root.join("game");
  let output = root.join("cache");
  let sibling = root.join("keep.txt");
  fs::create_dir_all(game.join("update")).unwrap();
  fs::create_dir_all(&output).unwrap();
  fs::write(output.join("stale.json"), "stale").unwrap();
  fs::write(&sibling, "keep").unwrap();
  let _cleanup = Staging(root.clone());

  let output = reset_output_directory(&output, &game.canonicalize().unwrap()).unwrap();
  assert!(output.is_dir());
  assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
  assert_eq!(fs::read_to_string(sibling).unwrap(), "keep");
  assert!(reset_output_directory(&game, &game.canonicalize().unwrap()).is_err());
  assert!(reset_output_directory(&root, &game.canonicalize().unwrap()).is_err());
  assert!(
    reset_output_directory(&std::env::current_dir().unwrap(), &game.canonicalize().unwrap())
      .is_err()
  );
}

#[test]
fn cache_manifest_requires_schema_one_and_root_metadata() {
  let root = std::env::temp_dir().join(format!("vanilla_schema_test_{}", std::process::id()));
  fs::create_dir_all(&root).unwrap();
  let _cleanup = Staging(root.clone());
  let mut manifest = VanillaCacheManifest {
    format_version: 1,
    game_dir: "game".into(),
    versions: vec![],
  };
  write_json(&root.join("cache_info.json"), &manifest).unwrap();
  assert_eq!(load_manifest(&root).unwrap().format_version, 1);
  manifest.format_version = 2;
  write_json(&root.join("cache_info.json"), &manifest).unwrap();
  assert!(
    load_manifest(&root).unwrap_err().to_string().contains("Unsupported GTA V cache schema 2")
  );
  fs::remove_file(root.join("cache_info.json")).unwrap();
  assert!(load_manifest(&root).is_err());
}

#[test]
fn stages_store_only_changed_raw_files_and_resolve_prior_versions() {
  init_test_version_logger();
  let root = std::env::temp_dir().join(format!("vanilla_cache_test_{}", std::process::id()));
  fs::create_dir_all(&root).unwrap();
  let _cleanup = Staging(root.clone());
  fs::write(root.join("first"), b"first map version").unwrap();
  fs::write(root.join("second"), b"second map version").unwrap();
  fs::write(root.join("third"), b"third map version").unwrap();
  let mut manifest = VanillaCacheManifest {
    format_version: 1,
    game_dir: "game".into(),
    versions: vec![],
  };
  let mut current = BTreeMap::new();
  let file = |name: &str, sha256: &str, stored: &str| {
    (
      ExtractedFile {
        name: name.into(),
        source: format!("nested.rpf/{name}"),
        stored: stored.into(),
        sha256: sha256.into(),
      },
      root.clone(),
    )
  };
  stage(&mut manifest, &mut current, &root, "base", || {
    Ok((vec![], vec![file("a.ymap", "111", "first"), file("b.ymap", "111", "first")]))
  })
  .unwrap();
  stage(&mut manifest, &mut current, &root, "dlc", || {
    Ok((
      vec![],
      vec![
        file("a.ymap", "222", "second"),
        file("b.ymap", "111", "first"),
        file("c.ymap", "222", "second"),
      ],
    ))
  })
  .unwrap();
  stage(&mut manifest, &mut current, &root, "empty", || Ok((vec![], vec![]))).unwrap();
  assert_eq!(manifest.versions[1].changes.len(), 2);
  assert_eq!(manifest.versions[1].changes["a.ymap"].previous_sha256.as_deref(), Some("111"));
  assert_eq!(manifest.versions[1].changes["c.ymap"].previous_sha256, None);
  assert!(manifest.versions[2].changes.is_empty());
  assert_eq!(manifest.resolve_version("0000-base").unwrap()["a.ymap"].sha256, "111");
  assert_eq!(manifest.resolve_version("0002-empty").unwrap()["a.ymap"].sha256, "222");
  assert_eq!(manifest.resolve_version("0002-empty").unwrap().len(), 3);
  assert!(manifest.resolve_version("unknown").is_err());
  assert_eq!(manifest.versions[1].unchanged, 1);
  assert!(root.join("0000-base/ymap/a.ymap").is_file());
  assert!(root.join("0001-dlc/ymap/a.ymap").is_file());
  assert!(!root.join("0001-dlc/ymap/b.ymap").exists());
  assert!(root.join("0001-dlc/ymap/c.ymap").is_file());
  assert_eq!(fs::read(root.join("0000-base/ymap/a.ymap")).unwrap(), b"first map version");
  assert_eq!(fs::read(root.join("0001-dlc/ymap/a.ymap")).unwrap(), b"second map version");
  assert_eq!(fs::read(root.join("0001-dlc/ymap/c.ymap")).unwrap(), b"second map version");
  for version in &manifest.versions {
    assert!(root.join(&version.id).join("version_info.json").is_file());
    assert!(root.join(&version.id).join("create_cache.log").is_file());
    assert!(root.join(&version.id).join("ymap").is_dir());
    assert!(!root.join(&version.id).join("native").exists());
  }
  let log = fs::read_to_string(root.join("0001-dlc/create_cache.log")).unwrap();
  assert!(log.contains("Modified a.ymap"));
  assert!(log.contains("Added c.ymap"));
  assert!(log.contains("Unchanged b.ymap"));
  assert!(
    !fs::read_to_string(root.join("0002-empty/create_cache.log"))
      .unwrap()
      .contains("Modified a.ymap")
  );
  stage(&mut manifest, &mut current, &root, "next", || {
    Ok((vec![], vec![file("a.ymap", "333", "third")]))
  })
  .unwrap();
  assert_eq!(manifest.versions[3].changes["a.ymap"].previous_sha256.as_deref(), Some("222"));
  assert_eq!(fs::read(root.join("0003-next/ymap/a.ymap")).unwrap(), b"third map version");
  let failed =
    stage(&mut manifest, &mut current, &root, "failed", || Err("test extraction failure".into()));
  assert!(failed.is_err());
  assert!(
    fs::read_to_string(root.join("0004-failed/create_cache.log"))
      .unwrap()
      .contains("test extraction failure")
  );
  let failures = preserve_failed_logs(&root, &root).unwrap();
  assert!(failures.join("0004-failed/create_cache.log").is_file());
}

#[test]
fn stages_cache_raw_ybn_replacements_and_ymap_files() {
  use crate::core::format::ybn::xml::xml_to_ybn;
  use sha2::{Digest, Sha256};

  let root = std::env::temp_dir().join(format!("gtav_ybn_stages_{}", std::process::id()));
  fs::create_dir(&root).unwrap();
  let _cleanup = Staging(root.clone());
  let before_xml = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/sample/ybn_conflicts/geometry_bvh.ybn.xml"
  ));
  let after_xml = before_xml.replacen("3, 0, 0\n", "3.1, 0, 0\n", 1);
  let before = xml_to_ybn(before_xml).unwrap();
  let after = xml_to_ybn(&after_xml).unwrap();
  fs::write(root.join("before.ybn"), &before).unwrap();
  fs::write(root.join("after.ybn"), &after).unwrap();
  fs::write(root.join("fake.ymap"), b"original map").unwrap();
  let file = |name: &str, stored: &str| {
    (
      ExtractedFile {
        name: name.into(),
        stored: stored.into(),
        source: format!("fixture.rpf/{name}"),
        sha256: format!("{:x}", Sha256::digest(fs::read(root.join(stored)).unwrap())),
      },
      root.clone(),
    )
  };
  let mut manifest = VanillaCacheManifest {
    format_version: 1,
    game_dir: "missing-game".into(),
    versions: vec![],
  };
  let mut current = BTreeMap::new();
  stage(&mut manifest, &mut current, &root, "base", || {
    Ok((vec![], vec![file("same.ybn", "before.ybn"), file("same.ymap", "fake.ymap")]))
  })
  .unwrap();
  stage(&mut manifest, &mut current, &root, "patch", || {
    Ok((vec![], vec![file("same.ybn", "after.ybn"), file("new.ybn", "after.ybn")]))
  })
  .unwrap();
  stage(&mut manifest, &mut current, &root, "unchanged", || {
    Ok((vec![], vec![file("same.ybn", "after.ybn")]))
  })
  .unwrap();
  assert_eq!(fs::read(root.join("0000-base/ybn/same.ybn")).unwrap(), before);
  assert!(root.join("0000-base/ymap/same.ymap").is_file());
  assert_eq!(fs::read(root.join("0001-patch/ybn/new.ybn")).unwrap(), after);
  assert_eq!(fs::read(root.join("0001-patch/ybn/same.ybn")).unwrap(), after);
  assert!(!root.join("0001-patch/ybn/same.ybn.diff.json").exists());
  assert!(manifest.versions[2].changes.is_empty());
  assert_eq!(manifest.versions[2].unchanged, 1);
  assert_eq!(manifest.resolve_version("0002-unchanged").unwrap().len(), 3);
  for version in &manifest.versions {
    assert!(root.join(&version.id).join("ybn").is_dir());
    assert!(!root.join(&version.id).join("native").exists());
    write_json(&root.join(&version.id).join("version_info.json"), version).unwrap();
  }
  write_json(&root.join("cache_info.json"), &manifest).unwrap();
  let listed = ListVanillaVersions {
    file_name: "SAME.YBN".into(),
    vanilla_cache_dir: root.clone(),
  }
  .versions()
  .unwrap();
  assert_eq!(listed.len(), 2);
  assert_eq!(listed[0].change, VanillaVersionChange::Added);
  assert_eq!(listed[1].change, VanillaVersionChange::Modified);
}

#[test]
fn discovers_all_installed_base_archives_without_assuming_a_final_letter() {
  let root = std::env::temp_dir().join(format!("gtav_base_discovery_{}", std::process::id()));
  fs::create_dir_all(root.join("directory.rpf")).unwrap();
  let _cleanup = Staging(root.clone());
  for name in ["x64z.rpf", "x64A.RPF", "common.rpf", "ignored.txt"] {
    fs::write(root.join(name), []).unwrap();
  }
  assert_eq!(base_archives(&root).unwrap(), ["common.rpf", "x64a.rpf", "x64z.rpf"]);
}

#[test]
#[ignore = "requires regenerated schema-1 cache under asset/vanilla-cache"]
fn real_ybn_cache_stores_raw_replacements() {
  use sha2::{Digest, Sha256};

  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache");
  let manifest = load_manifest(&root).unwrap();
  let (name, change) = manifest
    .versions
    .iter()
    .flat_map(|version| version.changes.iter())
    .find(|(name, change)| name.ends_with(".ybn") && change.previous_sha256.is_some())
    .expect("requires a YBN replacement");
  let file = &change.file;
  let raw = fs::read(root.join(&file.object)).unwrap();
  assert!(file.object.ends_with(".ybn"));
  assert_eq!(format!("{:x}", Sha256::digest(&raw)), file.sha256);
  let versions = ListVanillaVersions {
    file_name: name.clone(),
    vanilla_cache_dir: root.clone(),
  }
  .versions()
  .unwrap();
  assert!(versions.iter().any(|version| version.change == VanillaVersionChange::Modified));
  println!(
    "Verified raw YBN replacement {name} across {} cache stages ({} bytes)",
    manifest.versions.len(),
    raw.len()
  );
}

#[test]
fn publication_restores_existing_versions_if_root_metadata_cannot_be_replaced() {
  let root = std::env::temp_dir().join(format!("gtav_publish_test_{}", std::process::id()));
  let build = root.join("build");
  let output = root.join("output");
  fs::create_dir_all(build.join("0000-base/ymap")).unwrap();
  fs::create_dir_all(output.join("0000-base/ymap")).unwrap();
  let _cleanup = Staging(root);
  let version = CacheVersion {
    id: "0000-base".into(),
    parent: None,
    archives: vec![],
    changes: BTreeMap::new(),
    unchanged: 0,
  };
  let manifest = VanillaCacheManifest {
    format_version: 1,
    game_dir: "game".into(),
    versions: vec![version],
  };
  fs::write(output.join("0000-base/ymap/map.ymap"), b"old").unwrap();
  fs::write(build.join("0000-base/ymap/map.ymap"), b"new").unwrap();
  fs::write(output.join("rpf_names.json"), b"old names").unwrap();
  fs::write(build.join("rpf_names.json"), b"new names").unwrap();
  fs::write(output.join("hash_names.json"), b"old index").unwrap();
  fs::write(build.join("hash_names.json"), b"new index").unwrap();
  serde_json::to_writer(
    fs::File::create(output.join("0000-base/version_info.json")).unwrap(),
    &manifest.versions[0],
  )
  .unwrap();
  serde_json::to_writer(fs::File::create(build.join("cache_info.json")).unwrap(), &manifest)
    .unwrap();
  fs::create_dir(output.join("cache_info.json")).unwrap();
  assert!(publish_cache(&build, &output, &manifest).is_err());
  assert_eq!(fs::read(output.join("0000-base/ymap/map.ymap")).unwrap(), b"old");
  assert_eq!(fs::read(output.join("rpf_names.json")).unwrap(), b"old names");
  assert_eq!(fs::read(output.join("hash_names.json")).unwrap(), b"old index");
  assert_eq!(fs::read(build.join("0000-base/ymap/map.ymap")).unwrap(), b"new");
  fs::remove_dir(output.join("cache_info.json")).unwrap();
  publish_cache(&build, &output, &manifest).unwrap();
  assert_eq!(fs::read(output.join("0000-base/ymap/map.ymap")).unwrap(), b"new");
  assert_eq!(fs::read(output.join("rpf_names.json")).unwrap(), b"new names");
  assert_eq!(fs::read(output.join("hash_names.json")).unwrap(), b"new index");
  assert!(output.join("cache_info.json").is_file());
}

#[test]
fn title_update_patches_belong_to_their_dlc_stage() {
  assert_eq!(
    patch_dlc("update/update.rpf/dlc_patch/mpbeach/x64/levels/map.rpf/a.ymap"),
    Some("mpbeach")
  );
  assert_eq!(patch_dlc("update/update.rpf/x64/levels/map.rpf/a.ymap"), None);
  assert_eq!(
    platform_virtual_path("x64w.rpf/dlcpacks/mpbeach/dlc.rpf").as_deref(),
    Some("x64/dlcpacks/mpbeach/dlc.rpf")
  );
  assert_eq!(platform_virtual_path("x64n.rpf/levels/gta5/map.rpf"), None);
}
