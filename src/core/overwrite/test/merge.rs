use super::*;
use crate::core::format::gamefile::meta::{MetaResource, MetaSchemaCatalog};
use crate::core::format::gamefile::rsc7::Rsc7Resource;
use crate::core::format::gamefile::test_support::sample_ymap;
use crate::core::format::ymap::binary::{write_ymap, ymap_to_model};
use crate::core::format::ymap::compare::YmapComparisonDiff;
use crate::core::merge::Merge;
use crate::core::vanilla::{
  CacheVersion, CachedFile, FileChange, VanillaCacheManifest, write_json,
};

/// The sample YMAP's bytes and a catalog able to encode it.
fn sample_bytes() -> (Vec<u8>, MetaSchemaCatalog) {
  let sample =
    Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/parent_refs/vanilla_parent.ymap");
  let bytes = fs::read(sample).unwrap();
  let mut catalog = MetaSchemaCatalog::default();
  catalog.add_resource(&MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap());
  (bytes, catalog)
}

/// Writes a raw vanilla archive at `vanilla` whose only file is `map.ymap` with `bytes`.
fn write_vanilla(
  vanilla: &Path,
  bytes: &[u8],
) {
  let object = "0000-base/ymap/map.ymap";
  fs::create_dir_all(vanilla.join("0000-base/ymap")).unwrap();
  fs::write(vanilla.join(object), bytes).unwrap();
  write_json(
    &vanilla.join("cache_info.json"),
    &VanillaCacheManifest {
      format_version: 1,
      game_dir: vanilla.with_file_name("game"),
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
              sha256: hex::encode(Sha256::digest(bytes)),
              object: object.into(),
              source: "base.rpf/map.ymap".into(),
            },
          },
        )]),
      }],
    },
  )
  .unwrap();
}

#[test]
fn overwrites_skip_source_diffs_written_against_the_current_files() {
  let (bytes, catalog) = sample_bytes();
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
  write_vanilla(&vanilla, &bytes);
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
    serde_json::from_reader::<_, serde_json::Value>(
      fs::File::open(output.join("cache_info.json")).unwrap(),
    )
    .unwrap()["files"]["map.ymap"]["merged_at"]
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

#[test]
fn vanilla_overwrites_merge_after_the_sources() {
  use crate::core::config::matching;
  use crate::core::format::ymap::compare::YmapDiffContext;
  use struct_diff::Diff;
  let (bytes, catalog) = sample_bytes();
  let vanilla_model = sample_ymap("parent_refs/vanilla_parent.ymap");

  let root = std::env::temp_dir().join(format!("merge_vanilla_overwrite_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let resource = root.join("source/[maps]/resource");
  let vanilla = root.join("vanilla");
  let overwrite_dir = root.join("overwrite");
  let output = root.join("merged");
  fs::create_dir_all(resource.join("stream")).unwrap();
  fs::write(resource.join("fxmanifest.lua"), []).unwrap();
  write_vanilla(&vanilla, &bytes);
  // The source moves the first entity.
  let mut source = vanilla_model.clone();
  source.entities[0].position.x += 1.0;
  fs::write(
    resource.join("stream/map.ymap"),
    write_ymap(&source, &source.entities, &catalog).unwrap(),
  )
  .unwrap();
  // The overwrite sets a content flag and moves the first two entities.
  let mut edited = vanilla_model.clone();
  let bit = (!edited.content_flags).trailing_zeros();
  edited.content_flags |= 1 << bit;
  edited.entities[0].position.x += 2.0;
  edited.entities[1].position.x += 3.0;
  let diffs = vanilla_model
    .diff(
      &edited,
      &YmapDiffContext::new(matching::current(), (&vanilla_model, None), (&edited, None)),
    )
    .unwrap();
  let write_overwrite = |sha256: &str| {
    let overwrite = VanillaOverwrite {
      schema_version: VanillaOverwrite::SCHEMA_VERSION,
      vanilla: OverwriteFingerprint {
        sha256: sha256.into(),
        size: bytes.len() as u64,
      },
      diffs: diffs.clone(),
    };
    fs::create_dir_all(overwrite_dir.join("vanilla/latest/ymap")).unwrap();
    write_json(&overwrite_dir.join("vanilla/latest/ymap/Map.ymap.diff.json"), &overwrite).unwrap();
  };
  write_overwrite(&hex::encode(Sha256::digest(&bytes)));

  let merge = Merge {
    source_dir: root.join("source"),
    vanilla_cache_dir: root.join("vanilla-cache"),
    source_cache_dir: root.join("source-cache"),
    output_dir: output.clone(),
    overwrite_dir: Some(overwrite_dir.clone()),
    force: false,
  };
  let merged = || {
    ymap_to_model(&fs::read(output.join("ymap/map.ymap")).unwrap(), &catalog.hash_names).unwrap()
  };
  let merged_at = || {
    serde_json::from_reader::<_, serde_json::Value>(
      fs::File::open(output.join("cache_info.json")).unwrap(),
    )
    .unwrap()["files"]["map.ymap"]["merged_at"]
      .clone()
  };

  // The source's change to the first entity wins, as the first change does; the overwrite's
  // other changes apply.
  merge.run_after_caches(&vanilla).unwrap();
  let result = merged();
  assert_eq!(result.content_flags, edited.content_flags);
  assert_eq!(result.entities[0].position.x, source.entities[0].position.x);
  assert_eq!(result.entities[1].position.x, edited.entities[1].position.x);
  let duplicates = fs::read_to_string(output.join("duplicates.json")).unwrap();
  assert!(duplicates.contains("first_change_wins"), "{duplicates}");
  assert!(duplicates.contains("overwrite/vanilla/latest"), "{duplicates}");
  let applied = merged_at();
  merge.run_after_caches(&vanilla).unwrap();
  assert_eq!(merged_at(), applied);

  // One written against another vanilla file is not applied.
  write_overwrite("stale");
  merge.run_after_caches(&vanilla).unwrap();
  assert_eq!(merged().content_flags, vanilla_model.content_flags);

  // Without a source replacing the file, the overwrite alone is merged into it.
  write_overwrite(&hex::encode(Sha256::digest(&bytes)));
  // A resource that replaces no vanilla file.
  let empty = root.join("no-source/empty");
  fs::create_dir_all(empty.join("stream")).unwrap();
  fs::write(empty.join("fxmanifest.lua"), []).unwrap();
  let merge = Merge {
    source_dir: root.join("no-source"),
    source_cache_dir: root.join("no-source-cache"),
    ..merge
  };
  merge.run_after_caches(&vanilla).unwrap();
  let result = merged();
  assert_eq!(result.content_flags, edited.content_flags);
  assert_eq!(result.entities[0].position.x, edited.entities[0].position.x);
  assert!(fs::read_to_string(output.join("_omit.txt")).unwrap().is_empty());
  fs::remove_dir_all(root).unwrap();
}
