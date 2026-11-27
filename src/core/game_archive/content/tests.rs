use super::*;
use crate::{
  core::{
    format::gamefile::meta::{MetaDataBlock, MetaResource, jenk_hash},
    format::rpf::{RpfKeys, tests::fixture},
    game_archive::{Staging, VanillaCacheManifest, extract, stage, sub_pack_count},
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
fn clothing_ymts_are_classified_by_root_type_in_every_supported_container() {
  for build in [meta, pso, rbf] {
    assert_eq!(
      classify_ymt(&build("CPedVariationInfo")).unwrap(),
      FileClassification::Included {
        kind: Some("ped_variation")
      }
    );
    assert_eq!(
      classify_ymt(&build("CCreatureMetaData")).unwrap(),
      FileClassification::Included {
        kind: Some("creature_metadata")
      }
    );
    assert_eq!(classify_ymt(&build("CScenarioManifest")).unwrap(), FileClassification::Excluded);
  }
  assert_eq!(
    classify_ymt(b"<?xml version=\"1.0\"?><CPedVariationInfo/>").unwrap(),
    FileClassification::Included {
      kind: Some("ped_variation")
    }
  );
  assert_eq!(
    classify_ymt(b"<CCreatureMetaData/>").unwrap(),
    FileClassification::Included {
      kind: Some("creature_metadata")
    }
  );
  assert_eq!(
    classify_ymt(b"<CPedModelInfo__InitDataList/>").unwrap(),
    FileClassification::Excluded
  );
  for bytes in [b"PSIN".as_slice(), b"RSC7", b"RBF0", b"<CPedVariationInfo>", b"\xff\0"] {
    assert!(classify_ymt(bytes).is_err());
  }
  let mut truncated = rbf("CPedVariationInfo");
  truncated.truncate(9);
  assert!(classify_ymt(&truncated).is_err());
}

#[test]
fn clothing_extraction_keeps_raw_definitions_and_uses_vanilla_overlays() {
  let root = std::env::temp_dir().join(format!("vanilla_clothing_{}", std::process::id()));
  fs::create_dir_all(root.join("pack")).unwrap();
  let _cleanup = Staging(root.clone());
  let before = meta("CPedVariationInfo");
  let after = pso("CPedVariationInfo");
  let npc = rbf("CPedVariationInfo");
  let creature_metadata = meta("CCreatureMetaData");
  let nested = fixture(&[
    ("same.ymt", &after, false),
    ("npc.ymt", &npc, false),
    ("mp_creaturemetadata_m_test.ymt", &creature_metadata, false),
  ]);
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
  let clothing = FileSelection::new([ArchiveContent::Clothing]);
  let files =
    extract(&keys, &root, "pack/dlc.rpf", &scratch, None, &mut Default::default(), &clothing)
      .unwrap();
  assert_eq!(sub_pack_count(&files, &scratch).unwrap(), 2);
  assert!(files.iter().any(|file| file.source.ends_with("/common/data/dlclist.xml")));
  let output = root.join("archive");
  fs::create_dir(&output).unwrap();
  let mut manifest = VanillaCacheManifest {
    format_version: crate::core::game_archive::FORMAT_VERSION,
    contents: crate::core::game_archive::ArchiveContent::ALL.into(),
    game_dir: root.clone(),
    title_updates: Default::default(),
    versions: vec![],
  };
  let mut current = BTreeMap::new();
  let files: Vec<_> = files.into_iter().map(|file| (file, scratch.clone())).collect();
  stage(&mut manifest, &mut current, &output, "base", &clothing.extensions(), || {
    Ok((vec!["pack/dlc.rpf".into()], files.clone()))
  })
  .unwrap();
  assert_eq!(
    current.keys().map(String::as_str).collect::<Vec<_>>(),
    ["mp_creaturemetadata_m_test.ymt", "npc.ymt", "same.ymt"]
  );
  assert_eq!(fs::read(output.join("0000-base/ymt/same.ymt")).unwrap(), after);
  assert_eq!(fs::read(output.join("0000-base/ymt/npc.ymt")).unwrap(), npc);
  assert_eq!(
    fs::read(output.join("0000-base/ymt/mp_creaturemetadata_m_test.ymt")).unwrap(),
    creature_metadata
  );
  assert_eq!(
    manifest.versions[0].file_kinds,
    BTreeMap::from([
      ("mp_creaturemetadata_m_test.ymt".into(), "creature_metadata".into()),
      ("npc.ymt".into(), "ped_variation".into()),
      ("same.ymt".into(), "ped_variation".into()),
    ])
  );
  let serialized: serde_json::Value = serde_json::to_value(&manifest).unwrap();
  assert_eq!(
    serialized["versions"][0]["file_kinds"]["mp_creaturemetadata_m_test.ymt"],
    "creature_metadata"
  );
  assert_eq!(current["same.ymt"].source, "pack/dlc.rpf/nested.rpf/same.ymt");
  let patch =
    files.iter().find(|(file, _)| file.source == "pack/dlc.rpf/same.ymt").unwrap().clone();
  stage(&mut manifest, &mut current, &output, "unchanged", &clothing.extensions(), || {
    Ok((vec![], files))
  })
  .unwrap();
  assert!(manifest.versions[1].changes.is_empty());
  assert_eq!(manifest.versions[1].unchanged, 3);
  assert!(!output.join("0001-unchanged").exists());
  assert!(!output.join("0001-unchanged/ymt/same.ymt").exists());
  assert!(!output.join("0000-base/ymap").exists());
  let previous = current["same.ymt"].sha256.clone();
  stage(&mut manifest, &mut current, &output, "patch", &clothing.extensions(), || {
    Ok((vec![], vec![patch]))
  })
  .unwrap();
  assert_eq!(fs::read(output.join("0002-patch/ymt/same.ymt")).unwrap(), before);
  assert_eq!(manifest.versions[2].changes["same.ymt"].previous_sha256.as_ref(), Some(&previous));
  assert_eq!(manifest.resolve_version("0002-patch").unwrap().len(), 3);

  let map_files = extract(
    &keys,
    &root,
    "pack/dlc.rpf",
    &root.join("maps"),
    None,
    &mut Default::default(),
    &FileSelection::new([ArchiveContent::Map]),
  )
  .unwrap();
  let names = map_files.iter().map(|file| file.name.as_str()).collect::<Vec<_>>();
  assert!(names.contains(&"map.ymap") && names.contains(&"map.ybn"));
  assert!(!names.iter().any(|name| name.ends_with(".ymt")));

  let all_files = extract(
    &keys,
    &root,
    "pack/dlc.rpf",
    &root.join("all"),
    None,
    &mut Default::default(),
    &FileSelection::new(ArchiveContent::ALL),
  )
  .unwrap();
  let mut names = all_files
    .iter()
    .filter(|file| file.name.contains('.') && !file.name.ends_with(".xml"))
    .map(|file| (file.name.as_str(), file.kind))
    .collect::<Vec<_>>();
  names.sort();
  assert_eq!(
    names,
    [
      ("map.ybn", None),
      ("map.ymap", None),
      ("mp_creaturemetadata_m_test.ymt", Some("creature_metadata")),
      ("npc.ymt", Some("ped_variation")),
      ("same.ymt", Some("ped_variation")),
      ("same.ymt", Some("ped_variation")),
    ]
  );

  fs::write(root.join("pack/broken.rpf"), fixture(&[("npc.ymt", b"PSIN", false)])).unwrap();
  let error = extract(
    &keys,
    &root,
    "pack/broken.rpf",
    &root.join("broken"),
    None,
    &mut Default::default(),
    &clothing,
  )
  .unwrap_err();
  assert!(error.to_string().contains("npc.ymt"));
}
