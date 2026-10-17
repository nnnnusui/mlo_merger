use super::*;
use crate::core::format::gamefile::meta::jenk_hash;

#[test]
fn parses_four_vanilla_pso_ymaps() {
  for name in ["cs1_railwyc", "cs1_railwyc_long_0", "id2_17", "id2_17_strm_0"] {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
      .join(format!("asset/vanilla/0000-base/ymap/{name}.ymap"));
    let bytes = std::fs::read(&path).unwrap();
    let resource = PsoResource::parse(&bytes).unwrap();
    assert_eq!(resource.blocks[resource.root - 1].name, jenk_hash("CMapData"));
    assert!(resource.structures.contains_key(&jenk_hash("CMapData")));
    let xml = resource.to_xml(&HashMap::new()).unwrap();
    assert!(xml.contains("<CMapData>"));
    assert!(xml.contains(&format!("<name>{name}</name>")));
    assert!(!xml.contains("<error"));
    let model = crate::core::format::ymap::binary::ymap_to_model(&bytes, &HashMap::new()).unwrap();
    let expected: crate::core::format::ymap::model::Ymap =
      quick_xml::de::from_str::<crate::core::format::ymap::xml::XmlYmap>(&xml).unwrap().into();
    assert_eq!(model, expected);
    let ordered_entities =
      crate::core::format::ymap::binary::ymap_to_model(&bytes, &HashMap::new()).unwrap().entities;
    let parsed: crate::core::format::ymap::xml::XmlYmap = quick_xml::de::from_str(&xml).unwrap();
    let expected_entities: Vec<crate::core::format::ymap::model::YmapEntity> =
      parsed.entities.items.into_iter().map(Into::into).collect();
    assert_eq!(ordered_entities, expected_entities);
    assert_eq!(crate::core::vanilla::read_ymap(&path).unwrap(), expected);
    assert_eq!(
      crate::core::vanilla_cache::ymap_parent_hash(&path).unwrap(),
      crate::core::format::ymap::reference_hash(&expected.parent),
    );
    let rebuilt = resource.rebuild_xml(&xml).unwrap();
    assert_eq!(PsoResource::parse(&rebuilt).unwrap().to_xml(&HashMap::new()).unwrap(), xml);
  }
}

#[test]
fn template_rebuild_edits_flags_and_rejects_shape_changes() {
  let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("asset/vanilla/0000-base/ymap/cs1_railwyc.ymap");
  let bytes = std::fs::read(path).unwrap();
  let xml = PsoResource::parse(&bytes).unwrap().to_xml(&HashMap::new()).unwrap();
  let edited = xml.replacen("<flags value=\"2\"", "<flags value=\"3\"", 1);
  assert_ne!(edited, xml);
  let binary = PsoResource::parse(&bytes).unwrap().rebuild_xml(&edited).unwrap();
  assert_eq!(PsoResource::parse(&binary).unwrap().to_xml(&HashMap::new()).unwrap(), edited);
  let changed_shape = xml.replacen("</entities>", "<Item /></entities>", 1);
  let error = PsoResource::parse(&bytes).unwrap().rebuild_xml(&changed_shape).unwrap_err();
  assert!(error.to_string().contains("array length"));
}
