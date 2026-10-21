use super::*;

#[test]
#[ignore = "requires local vanilla YMAP schemas"]
fn incremental_ymap_merge_caches_noop_results_and_removes_obsolete_outputs() {
  use crate::core::format::gamefile::{
    meta::{MetaResource, MetaSchemaCatalog},
    rsc7::Rsc7Resource,
  };
  use crate::core::format::ymap::binary::ymap_to_model;
  let local = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap");
  let sample = fs::read_dir(local)
    .unwrap()
    .map(|entry| entry.unwrap().path())
    .find(|path| path.file_name().unwrap().to_string_lossy().contains("occl"))
    .unwrap();
  let bytes = fs::read(sample).unwrap();
  let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
  let mut catalog = MetaSchemaCatalog::default();
  catalog.add_resource(&meta);
  let model = ymap_to_model(&bytes, &catalog.hash_names).unwrap();
  let entities = model.entities.clone();
  let root = std::env::temp_dir().join(format!("incremental_ymap_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let source = root.join("source");
  let resource = source.join("resource");
  let vanilla = root.join("vanilla");
  let cache = root.join("vanilla-cache");
  let source_cache = root.join("source-cache");
  let output = root.join("merged");
  fs::create_dir_all(resource.join("stream")).unwrap();
  fs::write(resource.join("fxmanifest.lua"), []).unwrap();
  let object = "0000-base/ymap/map.ymap";
  fs::create_dir_all(vanilla.join("0000-base/ymap")).unwrap();
  fs::write(vanilla.join(object), &bytes).unwrap();
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
          "map.ymap".into(),
          FileChange {
            previous_sha256: None,
            file: CachedFile {
              sha256: hex::encode(Sha256::digest(&bytes)),
              object: object.into(),
              source: "base.rpf/map.ymap".into(),
            },
          },
        )]),
      }],
    },
  )
  .unwrap();
  let mut unsupported = model.clone();
  unsupported.flags ^= 1;
  let write_source = |model: &crate::core::format::ymap::model::Ymap| {
    fs::write(
      resource.join("stream/map.ymap"),
      crate::core::format::ymap::binary::write_ymap(model, &entities).unwrap(),
    )
    .unwrap();
  };
  let read_cache = || {
    serde_json::from_reader::<_, super::super::super::incremental::MergeMetadata>(
      fs::File::open(output.join("cache_info.json")).unwrap(),
    )
    .unwrap()
  };
  write_source(&unsupported);
  Merge {
    source_dir: source.clone(),
    vanilla_cache_dir: cache.clone(),
    source_cache_dir: source_cache.clone(),
    output_dir: output.clone(),
    force: false,
    overwrite_dir: None,
  }
  .run_after_caches(&vanilla)
  .unwrap();
  let initial = read_cache();
  assert!(initial.files["map.ymap"].output.is_none());
  assert!(!output.join("ymap/map.ymap").exists());
  Merge {
    source_dir: source.clone(),
    vanilla_cache_dir: cache.clone(),
    source_cache_dir: source_cache.clone(),
    output_dir: output.clone(),
    force: false,
    overwrite_dir: None,
  }
  .run_after_caches(&vanilla)
  .unwrap();
  assert_eq!(read_cache().files["map.ymap"].merged_at, initial.files["map.ymap"].merged_at);
  let mut edited = model.clone();
  let bit = (!edited.content_flags).trailing_zeros();
  assert!(bit < 32);
  edited.content_flags |= 1u32 << bit;
  write_source(&edited);
  Merge {
    source_dir: source.clone(),
    vanilla_cache_dir: cache.clone(),
    source_cache_dir: source_cache.clone(),
    output_dir: output.clone(),
    force: false,
    overwrite_dir: None,
  }
  .run_after_caches(&vanilla)
  .unwrap();
  assert!(output.join("ymap/map.ymap").is_file());
  let actual = fs::read(output.join("ymap/map.ymap")).unwrap();
  assert_eq!(
    ymap_to_model(&actual, &catalog.hash_names).unwrap().content_flags,
    edited.content_flags
  );
  assert_eq!(fs::read_to_string(output.join("_omit.txt")).unwrap(), "resource/stream/map.ymap");
  let changed = read_cache();
  Merge {
    source_dir: source.clone(),
    vanilla_cache_dir: cache.clone(),
    source_cache_dir: source_cache.clone(),
    output_dir: output.clone(),
    force: false,
    overwrite_dir: None,
  }
  .run_after_caches(&vanilla)
  .unwrap();
  assert_eq!(read_cache().files["map.ymap"].merged_at, changed.files["map.ymap"].merged_at);
  write_source(&unsupported);
  Merge {
    source_dir: source.clone(),
    vanilla_cache_dir: cache.clone(),
    source_cache_dir: source_cache.clone(),
    output_dir: output.clone(),
    force: false,
    overwrite_dir: None,
  }
  .run_after_caches(&vanilla)
  .unwrap();
  assert!(!output.join("ymap/map.ymap").exists());
  assert!(read_cache().files["map.ymap"].output.is_none());
  assert!(fs::read_to_string(output.join("_omit.txt")).unwrap().is_empty());
  fs::remove_dir_all(root).unwrap();
}
