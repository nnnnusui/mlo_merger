//! GTAV archive, stage, logging and publication regression tests.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use super::{
  archives::{
    DlcList, ExtractedFile, base_archives, dlc_archive, patch_dlc, platform_virtual_path,
  },
  publication::{Staging, preserve_failed_logs, publish_cache},
  stage::stage,
};
use crate::core::format::ymap::{model::Ymap, xml::XmlYmap};

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
fn stages_store_only_changes_and_resolve_prior_versions() {
  init_test_version_logger();
  let root = std::env::temp_dir().join(format!("gtav_cache_test_{}", std::process::id()));
  fs::create_dir_all(&root).unwrap();
  let _cleanup = Staging(root.clone());
  let xml: XmlYmap = quick_xml::de::from_str(include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/sample/parent_refs/vanilla_parent.ymap.xml"
  )))
  .unwrap();
  let before: Ymap = xml.into();
  let mut after = before.clone();
  after.parent = "changed_parent".into();
  fs::write(root.join("first"), serde_json::to_vec(&before).unwrap()).unwrap();
  fs::write(root.join("second"), serde_json::to_vec(&after).unwrap()).unwrap();
  let mut third = after.clone();
  third.parent = "next_parent".into();
  fs::write(root.join("third"), serde_json::to_vec(&third).unwrap()).unwrap();
  let reader =
    |path: &Path| -> Result<Ymap> { Ok(serde_json::from_reader(fs::File::open(path)?)?) };
  let mut manifest = GtavCacheManifest {
    format_version: 3,
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
  stage(
    &mut manifest,
    &mut current,
    &root,
    "base",
    || Ok((vec![], vec![file("a.ymap", "111", "first"), file("b.ymap", "111", "first")])),
    &reader,
  )
  .unwrap();
  stage(
    &mut manifest,
    &mut current,
    &root,
    "dlc",
    || {
      Ok((
        vec![],
        vec![
          file("a.ymap", "222", "second"),
          file("b.ymap", "111", "first"),
          file("c.ymap", "222", "second"),
        ],
      ))
    },
    &reader,
  )
  .unwrap();
  stage(&mut manifest, &mut current, &root, "empty", || Ok((vec![], vec![])), &reader).unwrap();
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
  assert!(root.join("0001-dlc/ymap/a.ymap.diff.json").is_file());
  assert!(
    manifest
      .versions
      .iter()
      .flat_map(|version| version.changes.values())
      .all(|change| change.file.native.is_none())
  );
  assert!(!root.join("0001-dlc/ymap/a.ymap").exists());
  assert!(!root.join("0001-dlc/ymap/b.ymap").exists());
  assert!(root.join("0001-dlc/ymap/c.ymap").is_file());
  let diff: ymap_delta::VanillaYmapDelta =
    serde_json::from_reader(fs::File::open(root.join("0001-dlc/ymap/a.ymap.diff.json")).unwrap())
      .unwrap();
  assert_eq!(diff.apply_to(&before).unwrap(), after);
  assert_eq!(
    fs::read(root.join("0001-dlc/ymap/c.ymap")).unwrap(),
    fs::read(root.join("second")).unwrap()
  );
  for version in &manifest.versions {
    assert!(root.join(&version.id).join("version_info.json").is_file());
    assert!(root.join(&version.id).join("create_cache.log").is_file());
    assert!(root.join(&version.id).join("ymap").is_dir());
    assert!(!root.join(&version.id).join("native").exists());
    let info: serde_json::Value = serde_json::from_reader(
      fs::File::open(root.join(&version.id).join("version_info.json")).unwrap(),
    )
    .unwrap();
    for change in info["changes"].as_object().unwrap().values() {
      assert!(change["file"].get("native").is_none());
    }
  }
  let delta: serde_json::Value =
    serde_json::from_reader(fs::File::open(root.join("0001-dlc/ymap/a.ymap.diff.json")).unwrap())
      .unwrap();
  assert!(delta["base"].get("native").is_none());
  let log = fs::read_to_string(root.join("0001-dlc/create_cache.log")).unwrap();
  assert!(log.contains("Diff a.ymap"));
  assert!(log.contains("Added c.ymap"));
  assert!(log.contains("Unchanged b.ymap"));
  assert!(
    !fs::read_to_string(root.join("0002-empty/create_cache.log")).unwrap().contains("Diff a.ymap")
  );
  stage(
    &mut manifest,
    &mut current,
    &root,
    "next",
    || Ok((vec![], vec![file("a.ymap", "333", "third")])),
    &reader,
  )
  .unwrap();
  assert_eq!(manifest.versions[3].changes["a.ymap"].previous_sha256.as_deref(), Some("222"));
  let diff: ymap_delta::VanillaYmapDelta =
    serde_json::from_reader(fs::File::open(root.join("0003-next/ymap/a.ymap.diff.json")).unwrap())
      .unwrap();
  assert_eq!(diff.apply_to(&after).unwrap(), third);
  let failed = stage(
    &mut manifest,
    &mut current,
    &root,
    "failed",
    || Err("test extraction failure".into()),
    &reader,
  );
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
  let manifest = GtavCacheManifest {
    format_version: 2,
    game_dir: "game".into(),
    versions: vec![version],
  };
  fs::write(output.join("0000-base/ymap/map.ymap"), b"old").unwrap();
  fs::write(build.join("0000-base/ymap/map.ymap"), b"new").unwrap();
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
  assert_eq!(fs::read(build.join("0000-base/ymap/map.ymap")).unwrap(), b"new");
  fs::remove_dir(output.join("cache_info.json")).unwrap();
  publish_cache(&build, &output, &manifest).unwrap();
  assert_eq!(fs::read(output.join("0000-base/ymap/map.ymap")).unwrap(), b"new");
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
