use super::*;
use crate::core::format::gamefile::test_support::sample_ymap;
use crate::core::format::ymap::binary::{write_ymap, ymap_to_model};
use crate::core::format::ymap::compare::YmapComparisonDiff;
use crate::core::overwrite::{OverwriteFingerprint, ResourceOverwrite, StreamFileOverwrite};

#[test]
fn overwrites_skip_source_diffs_written_against_the_current_files() {
  use crate::core::format::gamefile::{
    meta::{MetaResource, MetaSchemaCatalog},
    rsc7::Rsc7Resource,
  };
  let sample =
    Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/parent_refs/vanilla_parent.ymap");
  let bytes = fs::read(sample).unwrap();
  let mut catalog = MetaSchemaCatalog::default();
  catalog.add_resource(&MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap());
  let vanilla_model = sample_ymap("parent_refs/vanilla_parent.ymap");

  let root = std::env::temp_dir().join(format!("merge_overwrite_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let resource = root.join("source/[maps]/resource");
  let vanilla = root.join("vanilla");
  let source_cache = root.join("source-cache");
  let overwrite_dir = root.join("overwrite");
  let output = root.join("merged");
  fs::create_dir_all(resource.join("stream/maps")).unwrap();
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
  // The source sets a content flag and moves the first entity.
  let mut edited = vanilla_model.clone();
  let bit = (!edited.content_flags).trailing_zeros();
  edited.content_flags |= 1 << bit;
  edited.entities[0].position.x += 1.0;
  fs::write(
    resource.join("stream/maps/map.ymap"),
    write_ymap(&edited, &edited.entities, &catalog).unwrap(),
  )
  .unwrap();

  let merge = Merge {
    source_dir: root.join("source"),
    vanilla_cache_dir: root.join("vanilla-cache"),
    source_cache_dir: source_cache.clone(),
    output_dir: output.clone(),
    overwrite_dir: Some(overwrite_dir.clone()),
    force: false,
  };
  let merged = || {
    ymap_to_model(&fs::read(output.join("ymap/map.ymap")).unwrap(), &catalog.hash_names).unwrap()
  };
  let merged_at = || {
    serde_json::from_reader::<_, super::super::super::incremental::MergeMetadata>(
      fs::File::open(output.join("merge_cache_info.json")).unwrap(),
    )
    .unwrap()
    .files["map.ymap"]
      .merged_at
      .clone()
  };
  merge.run_after_caches(&vanilla).unwrap();
  assert_eq!(merged().content_flags, edited.content_flags);
  let unchanged = merged_at();

  let document: YmapComparisonDiff = serde_json::from_reader(
    fs::File::open(source_cache.join("diff/resource/maps/map.ymap.diff.json")).unwrap(),
  )
  .unwrap();
  let write_overwrite = |source_sha256: &str| {
    let overwrite = ResourceOverwrite {
      schema_version: ResourceOverwrite::SCHEMA_VERSION,
      files: BTreeMap::from([(
        "Maps/Map.ymap".into(),
        StreamFileOverwrite {
          source: OverwriteFingerprint {
            sha256: source_sha256.into(),
            size: document.source.size,
          },
          vanilla: OverwriteFingerprint {
            sha256: document.vanilla.sha256.clone(),
            size: document.vanilla.size,
          },
          skip: serde_json::json!({"content_flags": true}),
        },
      )]),
    };
    fs::create_dir_all(overwrite_dir.join("resources")).unwrap();
    write_json(&overwrite_dir.join("resources/resource.json"), &overwrite).unwrap();
  };

  // A changed overwrite merges again, without the skipped difference.
  write_overwrite(&document.source.sha256);
  merge.run_after_caches(&vanilla).unwrap();
  let skipped = merged();
  assert_eq!(skipped.content_flags, vanilla_model.content_flags);
  assert_eq!(skipped.entities[0].position.x, edited.entities[0].position.x);
  let applied = merged_at();
  assert_ne!(applied, unchanged);
  merge.run_after_caches(&vanilla).unwrap();
  assert_eq!(merged_at(), applied);

  // One written against another source file is not applied.
  write_overwrite("stale");
  merge.run_after_caches(&vanilla).unwrap();
  assert_eq!(merged().content_flags, edited.content_flags);
  fs::remove_dir_all(root).unwrap();
}
