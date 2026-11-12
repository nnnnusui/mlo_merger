use super::*;
use crate::core::format::ymap::binary::ymap_to_model;
use crate::platform::fs;
use entity::{FileResult, entity_data, search_result};
use inputs::matches;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;

#[test]
fn entity_position_search_includes_the_3d_boundary_and_all_occurrences() {
  use crate::common::position::Position;
  let model: crate::core::format::ymap::model::Ymap =
    crate::core::format::gamefile::test_support::sample_ymap("parent_refs/vanilla_parent.ymap");
  let sample = model.entities.first().unwrap();
  let positions = [
    [3.5, 4.2, 0.0],
    [4.5, 4.2, 0.0],
    [3.5, 4.2, 1.0],
    [3.5, 4.2, -1.0],
    [4.5001, 4.2, 0.0],
    [3.5, 4.2, 1.001],
    [4.5, 5.2, 0.0],
  ];
  let entities = positions
    .into_iter()
    .enumerate()
    .map(|(index, position)| {
      let mut entity = sample.clone();
      entity.guid = index as u32;
      entity.position = Position {
        x: position[0],
        y: position[1],
        z: position[2],
      };
      entity
    })
    .collect::<Vec<_>>();
  let query = EntityQuery::Position {
    position: [3.5, 4.2, 0.0],
    radius: 1.0,
  };
  query.validate().unwrap();
  let data = entity_data(Path::new("merged/map.ymap"), &query, "merged", None, entities.clone());
  assert_eq!(
    data.entities.iter().map(|entity| entity.guid.value).collect::<Vec<_>>(),
    [0, 1, 2, 3]
  );
  let exact = EntityQuery::Position {
    position: [3.5, 4.2, 0.0],
    radius: 0.0,
  };
  assert!(exact.contains(&entities[0]));
  assert!(!exact.contains(&entities[1]));
  for query in [
    EntityQuery::Position {
      position: [f64::NAN, 0.0, 0.0],
      radius: 1.0,
    },
    EntityQuery::Position {
      position: [f64::MAX, 0.0, 0.0],
      radius: 1.0,
    },
    EntityQuery::Position {
      position: [0.0; 3],
      radius: -1.0,
    },
    EntityQuery::Position {
      position: [0.0; 3],
      radius: f64::INFINITY,
    },
  ] {
    assert!(query.validate().is_err());
  }
}

#[test]
fn filename_globs_match_prefixed_ymaps_and_preserve_exact_filters() {
  let filter = filename_filter(Some("*cs4_10_strm_0.ymap")).unwrap();
  for name in [
    "hei_cs4_10_strm_0.ymap",
    "lr_cs4_10_strm_0.ymap",
    "cs4_10_strm_0.ymap",
    "LR_CS4_10_STRM_0.YMAP",
  ] {
    assert!(matches(&filter, name), "{name}");
  }
  assert!(!matches(&filter, "lr_cs4_10_strm_1.ymap"));
  assert!(!matches(&filter, "lr_cs4_10_strm_0.ybn"));
  let exact = filename_filter(Some("LR_CS4_10_STRM_0.YMAP")).unwrap();
  assert!(matches(&exact, "lr_cs4_10_strm_0.ymap"));
  assert!(!matches(&exact, "hei_cs4_10_strm_0.ymap"));
  assert!(matches(
    &filename_filter(Some("[hl]?_cs4_10_strm_0.ymap")).unwrap(),
    "lr_cs4_10_strm_0.ymap"
  ));
  assert!(filename_filter(Some("[invalid")).is_err());
  assert!(filename_filter(Some("../*.ymap")).is_err());
  assert!(filename_filter(Some("folder\\*.ymap")).is_err());
  assert!(matches(&filename_filter(None).unwrap(), "any.ymap"));
}

#[test]
fn entity_guid_xml_keeps_unsigned_guids_duplicates_and_stage_context() {
  let model: crate::core::format::ymap::model::Ymap =
    crate::core::format::gamefile::test_support::sample_ymap("parent_refs/vanilla_parent.ymap");
  let mut entity = model.entities.first().unwrap().clone();
  entity.guid = 2443198849;
  let data = entity_data(
    Path::new("merged/ymap/lr_cs4_10_strm_0.ymap"),
    &EntityQuery::Guid(entity.guid),
    "merged",
    None,
    vec![entity.clone(), entity],
  );
  assert!(data.found);
  assert_eq!(data.entities.len(), 2);
  let result = search_result(
    &EntityQuery::Guid(2443198849),
    vec![FileResult {
      name: "lr_cs4_10_strm_0.ymap".into(),
      data: vec![data],
    }],
  );
  let xml = quick_xml::se::to_string(&result).unwrap();
  assert!(xml.contains("stage=\"merged\""));
  assert_eq!(xml.matches("<guid value=\"2443198849\"").count(), 2);
  assert_eq!(xml.matches("<Item type=").count(), 2);
}

#[test]
#[ignore = "requires local vanilla YMAP schemas"]
fn find_searches_native_merged_and_recorded_before_entities_without_writes() {
  use crate::core::format::gamefile::{
    meta::{MetaResource, MetaSchemaCatalog},
    rsc7::Rsc7Resource,
  };
  let paths = crate::core::vanilla_cache::test_support::asset_latest_files("ymap");
  let bytes = fs::read(
    paths
      .iter()
      .find(|path| {
        ymap_to_model(&fs::read(path).unwrap(), &HashMap::new())
          .is_ok_and(|model| !model.entities.is_empty())
      })
      .unwrap(),
  )
  .unwrap();
  let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
  let mut catalog = MetaSchemaCatalog::default();
  catalog.add_resource(&meta);
  let model = ymap_to_model(&bytes, &catalog.hash_names).unwrap();
  let mut entities = model.entities.clone();
  entities[0].guid = 2443198849;
  let root = std::env::temp_dir().join(format!("find_entity_guid_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let merged = root.join("merged");
  fs::create_dir_all(merged.join("ymap")).unwrap();
  let vanilla = root.join("vanilla.ymap");
  let source = root.join("source.ymap");
  let merged_path = merged.join("ymap/lr_cs4_10_strm_0.ymap");
  let before = crate::core::format::ymap::binary::write_ymap(&model, &entities).unwrap();
  fs::write(&vanilla, &before).unwrap();
  fs::write(&source, &before).unwrap();
  entities[0].position.x += 1.0;
  fs::write(
    &merged_path,
    crate::core::format::ymap::binary::write_ymap(&model, &entities).unwrap(),
  )
  .unwrap();
  let input = |path: &Path, resource: Option<&str>| {
    serde_json::json!({
      "path": path, "resource": resource, "original_path": "resources/stream/lr_cs4_10_strm_0.ymap",
      "fingerprint": {"sha256": hex::encode(Sha256::digest(&before)), "size": before.len()},
    })
  };
  let record = serde_json::json!({"format_version": 1, "files": {
    "lr_cs4_10_strm_0.ymap": {"vanilla": input(&vanilla, None), "merge_sources": [input(&source, Some("resource"))]},
  }});
  crate::core::vanilla::write_json(&merged.join("cache_info.json"), &record).unwrap();
  let query = FindEntityGuid {
    guid: 2443198849,
    merged_dir: merged.clone(),
    filter: Some("LR_CS4_10_STRM_0.YMAP".into()),
    diff_all: false,
  };
  let modified = fs::metadata(&merged_path).unwrap().modified().unwrap();
  let xml = query.run().unwrap();
  assert!(xml.contains("stage=\"merged\""));
  assert!(!xml.contains("stage=\"source\""));
  let position = &entities[0].position;
  let nearby = FindEntity {
    query: EntityQuery::Position {
      position: [position.x as f64, position.y as f64, position.z as f64],
      radius: 1.0,
    },
    merged_dir: merged.clone(),
    stores: Default::default(),
    filter: query.filter.clone(),
    diff_all: true,
  };
  let nearby_xml = nearby.run().unwrap();
  assert!(nearby_xml.starts_with("<EntityPositionSearch"));
  assert!(nearby_xml.contains("position=\""));
  assert!(nearby_xml.contains("radius=\"1"));
  assert!(!nearby_xml.contains(" guid=\""));
  assert!(nearby_xml.contains("stage=\"vanilla\""));
  assert!(nearby_xml.contains("stage=\"source\""));
  assert_eq!(nearby_xml.matches("<guid value=\"2443198849\"").count(), 3);
  let file_xml = FindEntity {
    query: EntityQuery::FileFromPosition {
      position: [
        f64::from((model.entities_extents_min.x + model.entities_extents_max.x) / 2.0),
        f64::from((model.entities_extents_min.y + model.entities_extents_max.y) / 2.0),
        f64::from((model.entities_extents_min.z + model.entities_extents_max.z) / 2.0),
      ],
    },
    merged_dir: merged.clone(),
    stores: Default::default(),
    filter: query.filter.clone(),
    diff_all: true,
  }
  .run()
  .unwrap();
  assert!(file_xml.starts_with("<FileFromPositionSearch position=\""));
  assert!(file_xml.contains("type=\"ymap\" stage=\"merged\""));
  assert!(file_xml.contains("stage=\"vanilla\"") && file_xml.contains("stage=\"source\""));
  let mut all = query.clone();
  all.diff_all = true;
  let xml = all.run().unwrap();
  assert!(xml.contains("stage=\"vanilla\""));
  assert!(xml.contains("stage=\"source\""));
  assert!(xml.contains("resource=\"resource\""));
  assert_eq!(xml.matches("<guid value=\"2443198849\"").count(), 3);
  assert_eq!(fs::metadata(&merged_path).unwrap().modified().unwrap(), modified);
  let mut expanded = record.clone();
  for name in ["hei_cs4_10_strm_0.ymap", "cs4_10_strm_0.ymap"] {
    fs::write(merged.join("ymap").join(name), fs::read(&merged_path).unwrap()).unwrap();
    expanded["files"]
      .as_object_mut()
      .unwrap()
      .insert(name.into(), record["files"]["lr_cs4_10_strm_0.ymap"].clone());
  }
  crate::core::vanilla::write_json(&merged.join("cache_info.json"), &expanded).unwrap();
  let mut wildcard = query.clone();
  wildcard.filter = Some("*cs4_10_strm_0.ymap".into());
  assert_eq!(wildcard.run().unwrap().matches("<File ").count(), 3);
  wildcard.diff_all = true;
  let xml = wildcard.run().unwrap();
  assert_eq!(xml.matches("<File ").count(), 3);
  assert_eq!(xml.matches("stage=\"vanilla\"").count(), 3);
  assert_eq!(xml.matches("stage=\"source\"").count(), 3);
  let mut missing = query.clone();
  missing.guid = u32::MAX;
  assert!(!missing.run().unwrap().contains("<File"));
  missing.filter = Some("missing.ymap".into());
  assert!(!missing.run().unwrap().contains("<File"));
  missing.filter = Some("../outside.ymap".into());
  assert!(missing.run().is_err());
  fs::remove_file(&merged_path).unwrap();
  let xml = all.run().unwrap();
  assert!(xml.contains("stage=\"merged\" found=\"false\""));
  assert_eq!(xml.matches("<guid value=\"2443198849\"").count(), 2);
  fs::write(&source, b"changed source").unwrap();
  assert!(all.run().unwrap_err().to_string().contains("Pre-merge input changed"));
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn bounds_filter_uses_inclusive_sphere_aabb_intersection() {
  assert_eq!(
    aabb_intersects_radius([1.0, 0.0, 0.0], 1.0, [2.0, -1.0, -1.0], [3.0, 1.0, 1.0]),
    Some(true)
  );
  assert_eq!(
    aabb_intersects_radius([1.0, 1.0, 0.0], 1.0, [2.0, 2.0, -1.0], [3.0, 3.0, 1.0]),
    Some(false)
  );
  assert_eq!(aabb_intersects_radius([0.0; 3], 1.0, [2.0, 0.0, 0.0], [1.0, 1.0, 1.0]), None);
}
