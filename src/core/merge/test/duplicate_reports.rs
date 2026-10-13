use super::*;

#[test]
#[ignore = "requires local vanilla YMAP schemas"]
fn incremental_merge_preserves_and_restores_duplicate_reports() {
  use super::super::super::duplicates::DuplicateReport;
  use crate::core::format::gamefile::{
    meta::{MetaResource, MetaSchemaCatalog},
    rsc7::Rsc7Resource,
  };
  use crate::core::format::ymap::binary::ymap_to_model_with_entities;
  let local = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap");
  let mut paths =
    fs::read_dir(local).unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
  paths.sort();
  let sample = paths
    .iter()
    .find(|path| {
      ymap_to_model_with_entities(&fs::read(path).unwrap(), &std::collections::HashMap::new())
        .is_ok_and(|(model, entities)| {
          model.entities.len() == entities.len() && entities.iter().any(|entity| entity.guid != 0)
        })
    })
    .unwrap();
  let name = sample.file_name().unwrap().to_str().unwrap().to_owned();
  let bytes = fs::read(sample).unwrap();
  let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
  let mut catalog = MetaSchemaCatalog::default();
  catalog.add_resource(&meta);
  let (model, entities) = ymap_to_model_with_entities(&bytes, &catalog.hash_names).unwrap();
  let index = entities.iter().position(|entity| entity.guid != 0).unwrap();
  let guid = entities[index].guid;
  let root = std::env::temp_dir().join(format!("duplicate_merge_cache_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let source = root.join("source");
  let vanilla = root.join("vanilla");
  let cache = root.join("vanilla-cache");
  let source_cache = root.join("source-cache");
  let output = root.join("merged");
  let object = format!("0000-base/ymap/{name}");
  fs::create_dir_all(vanilla.join("0000-base/ymap")).unwrap();
  fs::write(vanilla.join(&object), &bytes).unwrap();
  write_json(
    &vanilla.join("cache_info.json"),
    &VanillaCacheManifest {
      format_version: 1,
      game_dir: root.join("game"),
      versions: vec![CacheVersion {
        id: "0000-base".into(),
        parent: None,
        archives: vec![],
        unchanged: 0,
        changes: BTreeMap::from([(
          name.clone(),
          FileChange {
            previous_sha256: None,
            file: CachedFile {
              sha256: format!("{:x}", Sha256::digest(&bytes)),
              object,
              source: "base.rpf".into(),
            },
          },
        )]),
      }],
    },
  )
  .unwrap();
  for (resource, offset) in [("resource_a", 1.0), ("resource_b", 2.0)] {
    let directory = source.join(resource);
    fs::create_dir_all(directory.join("stream")).unwrap();
    fs::write(directory.join("fxmanifest.lua"), []).unwrap();
    let mut edited = entities.clone();
    edited[index].position.x += offset;
    fs::write(
      directory.join("stream").join(&name),
      crate::core::format::ymap::binary::write_ymap(&model, &edited, &catalog).unwrap(),
    )
    .unwrap();
  }
  let run_merge = || {
    Merge {
      source_dir: source.clone(),
      vanilla_dir: vanilla.clone(),
      vanilla_cache_dir: cache.clone(),
      source_cache_dir: source_cache.clone(),
      output_dir: output.clone(),
      overwrite_dir: root.join("overwrite"),
      exclusions: None,
      force: false,
    }
    .run()
    .unwrap()
  };
  let read_report = || {
    serde_json::from_reader::<_, DuplicateReport>(
      fs::File::open(output.join("duplicates.json")).unwrap(),
    )
    .unwrap()
  };
  let read_cache = || {
    serde_json::from_reader::<_, super::super::super::incremental::MergeMetadata>(
      fs::File::open(output.join("merge_cache_info.json")).unwrap(),
    )
    .unwrap()
  };
  run_merge();
  let report = read_report();
  let duplicate = report.files[&name].iter().find(|entry| entry.guid == guid).unwrap();
  assert_eq!(duplicate.applied.resource, "resource_a");
  assert_eq!(duplicate.ignored[0].source.resource, "resource_b");
  assert_eq!(duplicate.ignored[0].reason, "first_change_wins");
  assert_eq!(duplicate.applied.entity.as_ref().unwrap().guid, guid);
  assert_eq!(duplicate.vanilla.as_ref().unwrap().guid, guid);
  let before = read_cache();
  let modified = fs::metadata(output.join("duplicates.json")).unwrap().modified().unwrap();
  run_merge();
  assert_eq!(read_report(), report);
  assert_eq!(fs::metadata(output.join("duplicates.json")).unwrap().modified().unwrap(), modified);
  fs::remove_file(output.join("duplicates.json")).unwrap();
  run_merge();
  assert_eq!(read_report(), report);
  assert_eq!(read_cache().files[&name].merged_at, before.files[&name].merged_at);
  fs::remove_dir_all(source.join("resource_b")).unwrap();
  run_merge();
  assert!(read_report().files.is_empty());
  fs::remove_dir_all(root).unwrap();
}
