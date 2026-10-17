use super::*;

pub(super) fn entity(
  guid: u32,
  lod_level: &str,
  parent_index: i32,
) -> YmapEntity {
  YmapEntity {
    entity_type: "CEntityDef".into(),
    archetype_name: "road".into(),
    flags: 0,
    guid,
    position: crate::common::position::Position::default(),
    rotation: crate::common::rotation::Rotation {
      x: 0.0,
      y: 0.0,
      z: 0.0,
      w: 1.0,
    },
    scale_x_y: 1.0,
    scale_z: 1.0,
    parent_index,
    lod_dist: 100.0,
    child_lod_dist: 100.0,
    lod_level: lod_level.into(),
    num_children: 0,
    priority_level: "PRI_REQUIRED".into(),
    ambient_occlusion_multiplier: 255,
    artificial_ambient_occlusion: 255,
    tint_value: 0,
  }
}

/// A map named `map` with `entities`, declaring the parent `external`.
pub(super) fn original_map(entities: Vec<YmapEntity>) -> OriginalMap {
  OriginalMap::from_model(Ymap {
    name: "map".into(),
    parent: "external".into(),
    entities,
    ..crate::core::format::gamefile::test_support::sample_ymap("parent_refs/vanilla_parent.ymap")
  })
}

#[test]
fn normalization_distinguishes_local_lod_and_explicit_external_parents() {
  let mut external = entity(30, "LODTYPES_DEPTH_HD", 0);
  external.flags = 8;
  let original = original_map(vec![
    entity(10, "LODTYPES_DEPTH_LOD", -1),
    entity(20, "LODTYPES_DEPTH_HD", 0),
    external,
  ]);
  let mut references = ParentReferences::default();
  let (_, normalized) =
    SourceMaps::default().normalize(&original, None, 123, &mut references).unwrap();
  let local = references.target(normalized[1].parent_index).unwrap();
  assert_eq!(local.map, 123);
  assert_eq!(local.guid, normalized[0].guid);
  assert_eq!(references.map_for(normalized[2].parent_index), Some(reference_hash("external")));
  assert_eq!(references.unavailable_index(normalized[2].parent_index), Some(0));
}

#[test]
fn indistinguishable_duplicates_receive_stable_occurrence_ids() {
  let first = entity(42, "LODTYPES_DEPTH_LOD", -1);
  let original = original_map(vec![first.clone(), first]);
  let mut references = ParentReferences::default();
  let (model, entities) =
    SourceMaps::default().normalize(&original, None, 123, &mut references).unwrap();
  assert_eq!(entities.len(), 2);
  assert_eq!(model.entities.len(), 2);
  assert_ne!(entities[0].guid, entities[1].guid);
  assert_eq!(references.original_guid(entities[0].guid).unwrap(), 42);
  assert_eq!(references.original_guid(entities[1].guid).unwrap(), 42);
  assert!(references.capture(123, &[entities[0].guid, entities[1].guid], 1).is_ok());
}

#[test]
fn mixed_mlo_arrays_resolve_runtime_indices_without_reordering_entities() {
  let mut mlo = entity(99, "LODTYPES_DEPTH_HD", -1);
  mlo.entity_type = "CMloInstanceDef".into();
  let original = original_map(vec![
    mlo,
    entity(10, "LODTYPES_DEPTH_LOD", -1),
    entity(20, "LODTYPES_DEPTH_HD", 0),
  ]);
  let mut references = ParentReferences::default();
  let (_, entities) =
    SourceMaps::default().normalize(&original, None, 123, &mut references).unwrap();
  let target = references.target(entities[2].parent_index).unwrap();
  assert_eq!(target.guid, entities[1].guid);
  let layout =
    runtime_entities(entities.iter()).into_iter().map(|entity| entity.guid).collect::<Vec<_>>();
  assert_eq!(references.restore(entities[2].parent_index, &layout).unwrap(), 0);
  assert_eq!(entities[0].entity_type, "CMloInstanceDef");
}

#[test]
#[ignore = "reads all local asset YMAP files and writes a validation report"]
fn validate_asset_ymap_readability() {
  use std::io::Write;

  let root = std::env::var_os("YMAP_VALIDATION_ROOT")
    .map(PathBuf::from)
    .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("asset"));
  let report = std::env::var_os("YMAP_VALIDATION_REPORT")
    .map(PathBuf::from)
    .unwrap_or_else(|| std::env::temp_dir().join("ymap-read-validation.json"));
  let mut paths = Vec::new();
  let mut failures = Vec::new();
  for entry in walkdir::WalkDir::new(&root) {
    match entry {
      Ok(entry) if entry.file_type().is_file() => {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.ends_with(".ymap") {
          paths.push(entry.into_path());
        }
      }
      Ok(_) => {}
      Err(error) => failures.push(serde_json::json!({
        "path": error.path(), "error": error.to_string(), "kind": "walk"
      })),
    }
  }
  paths.sort();
  let mut counts = std::collections::BTreeMap::<String, [usize; 2]>::new();
  let mut results = HashMap::<[u8; 32], Result<(), String>>::new();
  let mut passed = 0;
  for (index, path) in paths.iter().enumerate() {
    let group = path.strip_prefix(&root).unwrap().components().next().unwrap();
    let key = group.as_os_str().to_string_lossy().into_owned();
    let result = std::fs::read(path).map_err(|error| error.to_string()).and_then(|bytes| {
      use sha2::Digest;
      let key: [u8; 32] = sha2::Sha256::digest(&bytes).into();
      results
        .entry(key)
        .or_insert_with(|| {
          std::panic::catch_unwind(|| OriginalMap::load(path))
            .map_err(|payload| {
              let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic");
              format!("loader panicked: {message}")
            })
            .and_then(|result| result.map(|_| ()).map_err(|error| error.to_string()))
        })
        .clone()
    });
    let totals = counts.entry(key).or_default();
    totals[0] += 1;
    match result {
      Ok(_) => passed += 1,
      Err(error) => {
        totals[1] += 1;
        failures.push(serde_json::json!({
          "path": path, "error": error.to_string(),
          "kind": "binary"
        }));
      }
    }
    if (index + 1) % 1000 == 0 {
      eprintln!(
        "YMAP validation: {}/{} checked, {} failures",
        index + 1,
        paths.len(),
        failures.len()
      );
    }
  }
  let mut writer = std::io::BufWriter::new(std::fs::File::create(&report).unwrap());
  serde_json::to_writer_pretty(
    &mut writer,
    &serde_json::json!({
      "root": root, "checked": paths.len(), "passed": passed,
      "unique_contents": results.len(),
      "failure_count": failures.len(), "groups_checked_failed": counts, "failures": failures
    }),
  )
  .unwrap();
  writer.flush().unwrap();
  eprintln!(
    "YMAP validation: {} checked, {passed} passed, {} failures; report: {}",
    paths.len(),
    failures.len(),
    report.display()
  );
  assert!(!paths.is_empty(), "no YMAP files found under {}", root.display());
  assert!(failures.is_empty(), "YMAP read failures; see {}", report.display());
}
