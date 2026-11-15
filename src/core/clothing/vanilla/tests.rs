use super::*;
use crate::{
  core::{
    format::gamefile::meta::{MetaDataBlock, jenk_hash},
    format::rpf::{RpfKeys, tests::fixture},
    vanilla::{Staging, VanillaCacheManifest, extract, stage, sub_pack_count},
  },
  platform::fs,
};
use std::collections::BTreeMap;

fn meta(root: &str) -> Vec<u8> {
  MetaResource {
    root_block_index: 1,
    structures: vec![],
    enums: vec![],
    data_blocks: vec![MetaDataBlock {
      structure_name_hash: jenk_hash(root),
      data: vec![0; 4],
    }],
    name: None,
  }
  .to_rsc7(2)
  .unwrap()
  .encode()
  .unwrap()
}

fn pso(root: &str) -> Vec<u8> {
  let mut bytes = b"PSIN".to_vec();
  bytes.extend(12u32.to_be_bytes());
  bytes.extend([0; 4]);
  bytes.extend(b"PMAP");
  bytes.extend(32u32.to_be_bytes());
  bytes.extend(1u32.to_be_bytes());
  bytes.extend(1u16.to_be_bytes());
  bytes.extend(0u16.to_be_bytes());
  for word in [jenk_hash(root), 8, 0, 4] {
    bytes.extend(word.to_be_bytes());
  }
  bytes.extend(b"PSCH");
  bytes.extend(12u32.to_be_bytes());
  bytes.extend(0u32.to_be_bytes());
  bytes
}

fn rbf(root: &str) -> Vec<u8> {
  let mut bytes = b"RBF0\0\0".to_vec();
  bytes.extend(u16::try_from(root.len()).unwrap().to_le_bytes());
  bytes.extend(root.as_bytes());
  bytes.extend([0; 6]);
  bytes.extend([0xff, 0xff]);
  bytes
}

#[test]
fn clothing_is_selected_by_root_type_in_every_supported_container() {
  for build in [meta, pso, rbf] {
    assert!(is_ped_variation(&build("CPedVariationInfo")).unwrap());
    assert!(!is_ped_variation(&build("CScenarioManifest")).unwrap());
  }
  assert!(is_ped_variation(b"<?xml version=\"1.0\"?><CPedVariationInfo/>").unwrap());
  assert!(!is_ped_variation(b"<CPedModelInfo__InitDataList/>").unwrap());
  for bytes in [b"PSIN".as_slice(), b"RSC7", b"RBF0", b"<CPedVariationInfo>", b"\xff\0"] {
    assert!(is_ped_variation(bytes).is_err());
  }
  let mut truncated = rbf("CPedVariationInfo");
  truncated.truncate(9);
  assert!(is_ped_variation(&truncated).is_err());
}

#[test]
fn clothing_extraction_keeps_raw_definitions_and_uses_vanilla_overlays() {
  let root = std::env::temp_dir().join(format!("vanilla_clothing_{}", std::process::id()));
  fs::create_dir_all(root.join("pack")).unwrap();
  let _cleanup = Staging(root.clone());
  let before = meta("CPedVariationInfo");
  let after = pso("CPedVariationInfo");
  let npc = rbf("CPedVariationInfo");
  let nested = fixture(&[("same.ymt", &after, false), ("npc.ymt", &npc, false)]);
  fs::write(
    root.join("pack/dlc.rpf"),
    fixture(&[
      ("Same.YMT", &before, false),
      ("mp_m_freemode_01.ymt", b"<CScenarioManifest/>", false),
      ("map.ymap", b"raw map", false),
      ("map.ybn", b"raw collision", false),
      ("setup2.xml", b"<SSetupData><subPackCount value=\"2\"/></SSetupData>", false),
      ("common/data/dlclist.xml", b"<SMandatoryPacksData><Paths/></SMandatoryPacksData>", false),
      ("nested.rpf", &nested, false),
    ]),
  )
  .unwrap();
  let scratch = root.join("scratch");
  let keys = RpfKeys::unencrypted();
  let files =
    extract(&keys, &root, "pack/dlc.rpf", &scratch, None, &mut Default::default(), FILES).unwrap();
  assert_eq!(sub_pack_count(&files, &scratch).unwrap(), 2);
  assert!(files.iter().any(|file| file.source.ends_with("/common/data/dlclist.xml")));
  let output = root.join("archive");
  fs::create_dir(&output).unwrap();
  let mut manifest = VanillaCacheManifest {
    format_version: 1,
    game_dir: root.clone(),
    versions: vec![],
  };
  let mut current = BTreeMap::new();
  let files: Vec<_> = files.into_iter().map(|file| (file, scratch.clone())).collect();
  stage(&mut manifest, &mut current, &output, "base", FILES.extensions, || {
    Ok((vec!["pack/dlc.rpf".into()], files.clone()))
  })
  .unwrap();
  assert_eq!(current.keys().map(String::as_str).collect::<Vec<_>>(), ["npc.ymt", "same.ymt"]);
  assert_eq!(fs::read(output.join("0000-base/ymt/same.ymt")).unwrap(), after);
  assert_eq!(fs::read(output.join("0000-base/ymt/npc.ymt")).unwrap(), npc);
  assert_eq!(current["same.ymt"].source, "pack/dlc.rpf/nested.rpf/same.ymt");
  let patch =
    files.iter().find(|(file, _)| file.source == "pack/dlc.rpf/same.ymt").unwrap().clone();
  stage(&mut manifest, &mut current, &output, "unchanged", FILES.extensions, || {
    Ok((vec![], files))
  })
  .unwrap();
  assert!(manifest.versions[1].changes.is_empty());
  assert_eq!(manifest.versions[1].unchanged, 2);
  assert!(!output.join("0001-unchanged").exists());
  assert!(!output.join("0001-unchanged/ymt/same.ymt").exists());
  assert!(!output.join("0000-base/ymap").exists());
  let previous = current["same.ymt"].sha256.clone();
  stage(&mut manifest, &mut current, &output, "patch", FILES.extensions, || {
    Ok((vec![], vec![patch]))
  })
  .unwrap();
  assert_eq!(fs::read(output.join("0002-patch/ymt/same.ymt")).unwrap(), before);
  assert_eq!(manifest.versions[2].changes["same.ymt"].previous_sha256.as_ref(), Some(&previous));
  assert_eq!(manifest.resolve_version("0002-patch").unwrap().len(), 2);

  let map_files = extract(
    &keys,
    &root,
    "pack/dlc.rpf",
    &root.join("maps"),
    None,
    &mut Default::default(),
    FileSelection::MAPS,
  )
  .unwrap();
  let names = map_files.iter().map(|file| file.name.as_str()).collect::<Vec<_>>();
  assert!(names.contains(&"map.ymap") && names.contains(&"map.ybn"));
  assert!(!names.iter().any(|name| name.ends_with(".ymt")));

  fs::write(root.join("pack/broken.rpf"), fixture(&[("npc.ymt", b"PSIN", false)])).unwrap();
  let error = extract(
    &keys,
    &root,
    "pack/broken.rpf",
    &root.join("broken"),
    None,
    &mut Default::default(),
    FILES,
  )
  .unwrap_err();
  assert!(error.to_string().contains("npc.ymt"));
}
