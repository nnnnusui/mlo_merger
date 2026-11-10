use super::*;
use crate::core::format::gamefile::meta::{MetaResource, MetaSchemaCatalog};
use crate::core::format::gamefile::rsc7::Rsc7Resource;
use crate::core::format::gamefile::test_support::sample_ymap;
use crate::core::format::ymap::binary::{write_ymap, ymap_to_model};
use crate::core::format::ymap::compare::YmapComparisonDiff;
use crate::core::format::ymap::model::ymap::Ymap;
use crate::core::merge::Merge;
use crate::core::overwrite_cache::OverwriteWarnings;
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
  fs::write(resource.join("stream/maps/map.ymap"), write_ymap(&edited, &edited.entities).unwrap())
    .unwrap();

  let merge = Merge {
    source_dir: root.join("source"),
    vanilla_cache_dir: root.join("vanilla-cache"),
    source_cache_dir: source_cache.clone(),
    output_dir: output.clone(),
    overwrite_dir: Some(overwrite_dir.clone()),
    overwrite_cache_dir: root.join("overwrite-cache"),
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
  let overwrite_path = overwrite_dir.join("resources/resource/Maps/Map.ymap.diff.json");
  let write_overwrite = |source_sha256: &str| {
    let overwrite = StreamFileOverwrite {
      schema_version: StreamFileOverwrite::SCHEMA_VERSION,
      ignore: false,
      source: Some(OverwriteFingerprint {
        sha256: source_sha256.into(),
        size: document.source.size,
      }),
      vanilla: Some(OverwriteFingerprint {
        sha256: document.vanilla.sha256.clone(),
        size: document.vanilla.size,
      }),
      skip: Some(serde_json::json!({"content_flags": true})),
    };
    fs::create_dir_all(overwrite_path.parent().unwrap()).unwrap();
    write_json(&overwrite_path, &overwrite).unwrap();
  };

  // A changed overwrite merges again, without the skipped difference.
  write_overwrite(&document.source.sha256);
  assert!(merge.run_after_caches(&vanilla).unwrap().warnings.ignored_overwrites.is_empty());
  let skipped = merged();
  assert_eq!(skipped.content_flags, vanilla_model.content_flags);
  assert_eq!(skipped.entities[0].position.x, edited.entities[0].position.x);
  let applied = merged_at();
  assert_ne!(applied, unchanged);
  merge.run_after_caches(&vanilla).unwrap();
  assert_eq!(merged_at(), applied);

  // One written against another source file is not applied, and is reported on every run,
  // including one that reuses the merged output.
  write_overwrite("stale");
  let stale = vec![IgnoredOverwrite {
    resource: Some("resource".into()),
    file: "maps/map.ymap".into(),
    reason: IgnoredOverwriteReason::SourceChanged,
  }];
  assert_eq!(merge.run_after_caches(&vanilla).unwrap().warnings.ignored_overwrites, stale);
  assert_eq!(merged().content_flags, edited.content_flags);
  let reused = merge.run_after_caches(&vanilla).unwrap();
  assert!(reused.changes.added.is_empty() && reused.changes.modified.is_empty());
  assert_eq!(reused.warnings.ignored_overwrites, stale);

  // One naming a stream file the resource does not merge is reported too.
  fs::create_dir_all(overwrite_dir.join("resources/resource/maps")).unwrap();
  fs::rename(&overwrite_path, overwrite_dir.join("resources/resource/maps/other.ymap.diff.json"))
    .unwrap();
  assert_eq!(
    merge.run_after_caches(&vanilla).unwrap().warnings.ignored_overwrites,
    vec![IgnoredOverwrite {
      resource: Some("resource".into()),
      file: "maps/other.ymap".into(),
      reason: IgnoredOverwriteReason::MissingSource,
    }]
  );

  // One of a format overwrites do not support yet is reported as such, even one ignoring its
  // file.
  fs::remove_file(overwrite_dir.join("resources/resource/maps/other.ymap.diff.json")).unwrap();
  write_json(
    &overwrite_dir.join("resources/resource/maps/map.ybn.diff.json"),
    &StreamFileOverwrite {
      schema_version: StreamFileOverwrite::SCHEMA_VERSION,
      ignore: true,
      source: None,
      vanilla: None,
      skip: None,
    },
  )
  .unwrap();
  assert_eq!(
    merge.run_after_caches(&vanilla).unwrap().warnings.ignored_overwrites,
    vec![IgnoredOverwrite {
      resource: Some("resource".into()),
      file: "maps/map.ybn".into(),
      reason: IgnoredOverwriteReason::UnsupportedFormat,
    }]
  );
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn overwrites_follow_their_source_to_its_next_version() {
  use crate::core::overwrite_cache::{SourceUpdate, SourceUpdateOutcome};

  let (bytes, catalog) = sample_bytes();
  let vanilla_model = sample_ymap("parent_refs/vanilla_parent.ymap");
  let root = std::env::temp_dir().join(format!("merge_overwrite_update_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let resource = root.join("source/resource");
  let vanilla = root.join("vanilla");
  let source_cache = root.join("source-cache");
  let overwrite_dir = root.join("overwrite");
  let overwrite_cache = root.join("overwrite-cache");
  let output = root.join("merged");
  fs::create_dir_all(resource.join("stream")).unwrap();
  fs::write(resource.join("fxmanifest.lua"), []).unwrap();
  write_vanilla(&vanilla, &bytes);
  let ship = |model: &Ymap| {
    fs::write(resource.join("stream/map.ymap"), write_ymap(model, &model.entities).unwrap())
      .unwrap();
  };
  let added = |guid: u32, y: f32| {
    let mut entity = vanilla_model.entities[0].clone();
    entity.guid = guid;
    entity.position.y += y;
    entity
  };
  // The resource moves vanilla entity 0 and adds entities 1000 and 2000.
  let mut first = vanilla_model.clone();
  first.entities[0].position.x += 1.0;
  first.entities.extend([added(1000, 10.0), added(2000, 20.0)]);
  ship(&first);

  let merge = Merge {
    source_dir: root.join("source"),
    vanilla_cache_dir: root.join("vanilla-cache"),
    source_cache_dir: source_cache.clone(),
    output_dir: output.clone(),
    overwrite_dir: Some(overwrite_dir.clone()),
    overwrite_cache_dir: overwrite_cache.clone(),
    force: false,
  };
  let merged = || {
    ymap_to_model(&fs::read(output.join("ymap/map.ymap")).unwrap(), &catalog.hash_names).unwrap()
  };
  let guids = |model: &Ymap| model.entities.iter().map(|entity| entity.guid).collect::<Vec<_>>();
  merge.run_after_caches(&vanilla).unwrap();
  let document: YmapComparisonDiff = serde_json::from_reader(
    fs::File::open(source_cache.join("diff/resource/map.ymap.diff.json")).unwrap(),
  )
  .unwrap();
  // The overwrite reverts the move and entity 1000.
  let overwrite_path = overwrite_dir.join("resources/resource/map.ymap.diff.json");
  let written = StreamFileOverwrite {
    schema_version: StreamFileOverwrite::SCHEMA_VERSION,
    ignore: false,
    source: Some(OverwriteFingerprint {
      sha256: document.source.sha256.clone(),
      size: document.source.size,
    }),
    vanilla: Some(OverwriteFingerprint {
      sha256: document.vanilla.sha256.clone(),
      size: document.vanilla.size,
    }),
    skip: Some(serde_json::json!({"entities": [
      {"Modified": {"before_index": 0, "changes": {"position": true}}},
      {"Added": {"entity": {"guid": 1000}}}
    ]})),
  };
  fs::create_dir_all(overwrite_path.parent().unwrap()).unwrap();
  write_json(&overwrite_path, &written).unwrap();
  merge.run_after_caches(&vanilla).unwrap();
  assert_eq!(guids(&merged()), [100, 200, 2000]);
  assert_eq!(merged().entities[0].position.x, vanilla_model.entities[0].position.x);
  let record = |sha256: &str| -> SourceUpdate {
    serde_json::from_reader(
      fs::File::open(
        overwrite_cache.join(format!("source_updates/resource/map.ymap/{sha256}.json")),
      )
      .unwrap(),
    )
    .unwrap()
  };

  // The next version no longer adds entity 1000, and adds entity 3000: the overwrite is rewritten
  // against it, without the selector of entity 1000, and keeps reverting the move.
  let mut second = first.clone();
  second.entities.retain(|entity| entity.guid != 1000);
  second.entities.push(added(3000, 30.0));
  ship(&second);
  let rewritten = merge.run_after_caches(&vanilla).unwrap();
  assert!(rewritten.warnings.is_empty(), "{:?}", rewritten.warnings);
  assert_eq!(guids(&merged()), [100, 200, 2000, 3000]);
  assert_eq!(merged().entities[0].position.x, vanilla_model.entities[0].position.x);
  let before = &document.source.sha256;
  // The source cache keeps the earlier version, with its diff document.
  assert!(source_cache.join(format!("history/resource/map.ymap/{before}.ymap")).is_file());
  assert!(source_cache.join(format!("history/resource/map.ymap/{before}.diff.json")).is_file());
  let update = record(before);
  assert_eq!(update.outcome, SourceUpdateOutcome::Rewritten);
  assert_eq!(update.overwrite, written);
  assert_eq!(update.before.diff, Some(format!("history/resource/map.ymap/{before}.diff.json")));
  assert_eq!(update.after.diff, Some("diff/resource/map.ymap.diff.json".into()));
  let reverted = serde_json::to_value(&update.reverted).unwrap()["entities"].clone();
  assert_eq!(reverted.as_array().unwrap().len(), 2, "{reverted}");
  let on_disk: StreamFileOverwrite =
    serde_json::from_reader(fs::File::open(&overwrite_path).unwrap()).unwrap();
  assert_eq!(Some(&on_disk), update.rewritten.as_ref());
  assert_eq!(
    on_disk.skip,
    Some(serde_json::json!({"entities": [
      {"Modified": {"before_index": 0, "changes": {"position": true}}}
    ]}))
  );
  assert_ne!(on_disk.source, written.source);
  // `--read --version` reads the earlier version from the history.
  let read = crate::core::read::Read {
    target: crate::core::read::ReadTarget::Resource("resource".into()),
    files: vec!["map.ymap".into()],
    source_dir: root.join("source"),
    source_cache_dir: source_cache.clone(),
    vanilla_dir: vanilla.clone(),
    vanilla_cache_dir: root.join("vanilla-cache"),
    gamebuild: None,
    output_dir: root.join("read-cache"),
    force: false,
    version: Some(before.clone()),
  };
  let result = read.run().unwrap();
  assert_eq!(
    result.files[0].origin,
    crate::core::read::ReadOrigin::History {
      resource: "resource".into(),
      stream_path: "map.ymap".into(),
      sha256: before.clone(),
    }
  );
  let document: crate::core::read::YmapReadDocument =
    serde_json::from_reader(fs::File::open(&result.files[0].output).unwrap()).unwrap();
  assert_eq!(guids(&document.model), [100, 200, 1000, 2000]);
  assert!(
    crate::core::read::Read {
      version: Some("missing".into()),
      ..read.clone()
    }
    .run()
    .is_err()
  );

  // The version after moves entity 0 elsewhere: the move it reverted changed, so it is left to
  // be written again, and not applied.
  let mut third = second.clone();
  third.entities[0].position.x += 5.0;
  ship(&third);
  let manual = merge.run_after_caches(&vanilla).unwrap();
  assert_eq!(manual.warnings.ignored_overwrites.len(), 1);
  assert_eq!(merged().entities[0].position.x, third.entities[0].position.x);
  let update = record(&on_disk.source.as_ref().unwrap().sha256);
  assert_eq!(update.outcome, SourceUpdateOutcome::Manual);
  assert!(
    update.reasons.iter().any(|reason| reason.contains("differs now")),
    "{:?}",
    update.reasons
  );
  let unchanged: StreamFileOverwrite =
    serde_json::from_reader(fs::File::open(&overwrite_path).unwrap()).unwrap();
  assert_eq!(unchanged, on_disk);
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
  // The source moves the first entity and changes the extents, which merging leaves out of a
  // resource's diff.
  let mut source = vanilla_model.clone();
  source.entities[0].position.x += 1.0;
  source.streaming_extents_max.x += 10.0;
  source.entities_extents_min.y -= 10.0;
  fs::write(resource.join("stream/map.ymap"), write_ymap(&source, &source.entities).unwrap())
    .unwrap();
  // The overwrite sets a content flag and moves the first two entities.
  let mut edited = vanilla_model.clone();
  let bit = (!edited.content_flags).trailing_zeros();
  edited.content_flags |= 1 << bit;
  edited.entities[0].position.x += 2.0;
  edited.entities[1].position.x += 3.0;
  // It also writes the extents, which apply from a vanilla overwrite.
  edited.streaming_extents_max.x += 20.0;
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
    overwrite_cache_dir: root.join("overwrite-cache"),
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
  assert_eq!(result.streaming_extents_max, edited.streaming_extents_max);
  assert_eq!(result.entities_extents_min, vanilla_model.entities_extents_min);
  let duplicates = fs::read_to_string(output.join("duplicates.json")).unwrap();
  assert!(duplicates.contains("first_change_wins"), "{duplicates}");
  assert!(duplicates.contains("overwrite/vanilla/latest"), "{duplicates}");
  let applied = merged_at();
  merge.run_after_caches(&vanilla).unwrap();
  assert_eq!(merged_at(), applied);

  // One written against another vanilla file is not applied.
  write_overwrite("stale");
  assert_eq!(
    merge.run_after_caches(&vanilla).unwrap().warnings.ignored_overwrites,
    vec![IgnoredOverwrite {
      resource: None,
      file: "map.ymap".into(),
      reason: IgnoredOverwriteReason::VanillaChanged,
    }]
  );
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

#[test]
fn added_ymaps_merge_every_version_without_shared_additions() {
  let (bytes, catalog) = sample_bytes();
  let sample = sample_ymap("parent_refs/vanilla_parent.ymap");

  let root = std::env::temp_dir().join(format!("merge_added_ymaps_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let vanilla = root.join("vanilla");
  let source_cache = root.join("source-cache");
  let overwrite_dir = root.join("overwrite");
  let overwrite_cache = root.join("overwrite-cache");
  let output = root.join("merged");
  write_vanilla(&vanilla, &bytes);
  let write_source = |resource: &str, name: &str, model: &Ymap| {
    let directory = root.join("source").join(resource);
    fs::create_dir_all(directory.join("stream")).unwrap();
    fs::write(directory.join("fxmanifest.lua"), []).unwrap();
    fs::write(directory.join("stream").join(name), write_ymap(model, &model.entities).unwrap())
      .unwrap();
  };
  // Three resources add `custom.ymap` with entities 100 and 200. The first lists them in the
  // other order, places entity 100 rotated as entity 150, and has an entity without a GUID that
  // the third has too; the second moves entity 100, adds entity 1200, places entity 1250 as
  // entity 200 at another LOD level and widens the extents; the third adds entity 1300. The
  // first also adds `solo.ymap`, whose entity's parent is entity 150 of its `custom.ymap`.
  let mut first = sample.clone();
  first.name = "custom".into();
  let mut no_guid = first.entities[0].clone();
  no_guid.guid = 0;
  no_guid.position.y += 30.0;
  let mut third = first.clone();
  first.entities[0].guid = 150;
  first.entities[0].rotation.z = 0.5;
  first.entities.reverse();
  first.entities.push(no_guid.clone());
  let mut second = third.clone();
  second.entities[0].position.x += 50.0;
  let mut added = second.entities[1].clone();
  added.guid = 1200;
  added.position.y += 20.0;
  second.entities.push(added);
  let mut other_lod = second.entities[1].clone();
  other_lod.guid = 1250;
  other_lod.lod_level = "LODTYPES_DEPTH_LOD".into();
  second.entities.push(other_lod);
  second.streaming_extents_max.x += 50.0;
  let mut extra = third.entities[1].clone();
  extra.guid = 1300;
  extra.position.y += 40.0;
  third.entities.push(extra);
  third.entities.push(no_guid);
  let mut solo = sample_ymap("parent_refs/child.ymap");
  solo.name = "solo".into();
  solo.parent = "custom".into();
  assert_eq!(first.entities[solo.entities[0].parent_index as usize].guid, 150);
  write_source("resource_a", "custom.ymap", &first);
  write_source("resource_a", "solo.ymap", &solo);
  write_source("resource_b", "custom.ymap", &second);
  write_source("resource_c", "custom.ymap", &third);

  let merge = Merge {
    source_dir: root.join("source"),
    vanilla_cache_dir: root.join("vanilla-cache"),
    source_cache_dir: source_cache.clone(),
    output_dir: output.clone(),
    overwrite_dir: Some(overwrite_dir.clone()),
    overwrite_cache_dir: overwrite_cache.clone(),
    force: false,
  };
  let read = |path: &Path| ymap_to_model(&fs::read(path).unwrap(), &catalog.hash_names).unwrap();
  let guids = |model: &Ymap| model.entities.iter().map(|entity| entity.guid).collect::<Vec<_>>();
  let merged = || read(&output.join("ymap/custom.ymap"));
  let added_stream_files = || -> serde_json::Value {
    serde_json::from_reader(
      fs::File::open(overwrite_cache.join("added_stream_files.json")).unwrap(),
    )
    .unwrap()
  };
  let document = |path: &Path| -> YmapComparisonDiff {
    serde_json::from_reader(fs::File::open(path).unwrap()).unwrap()
  };
  let write_overwrite = |path: &str, overwrite: StreamFileOverwrite| {
    let path = overwrite_dir.join("resources").join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    write_json(&path, &overwrite).unwrap();
  };
  // A file of an earlier format of the overwrite cache is removed when it is rebuilt.
  fs::create_dir_all(&overwrite_cache).unwrap();
  fs::write(overwrite_cache.join("added_ymaps.json"), b"{}").unwrap();
  let first_run = merge.run_after_caches(&vanilla).unwrap();
  assert!(!overwrite_cache.join("added_ymaps.json").exists());

  // The added YMAPs are moved into the source cache and compared with an empty YMAP.
  assert!(source_cache.join("resources/resource_a/custom.ymap").is_file());
  let relationships: serde_json::Value = serde_json::from_reader(
    fs::File::open(source_cache.join("added_ymap_relationships.json")).unwrap(),
  )
  .unwrap();
  assert_eq!(
    relationships["children_by_parent"]["resource_a/custom.ymap"],
    serde_json::json!(["resource_a/solo.ymap"])
  );
  let solo_document = document(&source_cache.join("diff/resource_a/solo.ymap.diff.json"));
  assert_eq!(solo_document.vanilla, crate::core::source_cache::empty_baseline_file().unwrap());

  // Every version is merged, the one loaded last first: an entity an earlier version adds
  // already, by GUID or as placed, is left out, so the third's entity 100 wins over the
  // second's and the first's entity 150. The extents cover every version's.
  let model = merged();
  assert_eq!(guids(&model), [100, 200, 1300, 0, 1200, 1250]);
  assert_eq!(model.entities[0].position.x, third.entities[0].position.x);
  assert_eq!(model.streaming_extents_max.x, second.streaming_extents_max.x);
  let files = added_stream_files();
  assert_eq!(
    files["files"]["custom.ymap"]["sources"]
      .as_array()
      .unwrap()
      .iter()
      .map(|source| source["resource"].as_str().unwrap())
      .collect::<Vec<_>>(),
    ["resource_c", "resource_b", "resource_a"]
  );
  assert_eq!(
    files["files"]["custom.ymap"]["shipped_by"],
    serde_json::json!(["resource_a", "resource_b", "resource_c"])
  );
  // The merge's duplicate report leaves them to the overwrite cache, which records each entity
  // left out by its resource and index, with the one kept in its place.
  let duplicates: serde_json::Value =
    serde_json::from_reader(fs::File::open(output.join("duplicates.json")).unwrap()).unwrap();
  assert!(duplicates["files"].get("custom.ymap").is_none(), "{duplicates}");
  let entity_left_out = files["files"]["custom.ymap"]["left_out"]
    .as_array()
    .unwrap()
    .iter()
    .filter(|left_out| left_out["field"] == "entities")
    .map(|left_out| {
      let name = |item: &serde_json::Value| {
        format!("{}[{}]", item["resource"].as_str().unwrap(), item["index"])
      };
      (name(left_out), name(&left_out["kept"]))
    })
    .collect::<Vec<_>>();
  let pair = |left_out: &str, kept: &str| (left_out.to_string(), kept.to_string());
  assert_eq!(
    entity_left_out,
    [
      pair("resource_b[0]", "resource_c[0]"),
      pair("resource_b[1]", "resource_c[1]"),
      pair("resource_a[0]", "resource_c[1]"),
      pair("resource_a[1]", "resource_c[0]"),
      pair("resource_a[2]", "resource_c[3]"),
    ]
  );
  // The child's parent index follows entity 150 to entity 100, which stands in for it.
  let solo_merged = read(&output.join("ymap/solo.ymap"));
  assert_eq!(guids(&solo_merged), [300]);
  assert_eq!(solo_merged.entities[0].parent_index, 0);
  // Versions of a YMAP several resources add are no warning; the cache records what the merge
  // reports.
  assert!(first_run.warnings.is_empty(), "{:?}", first_run.warnings);
  let warnings: OverwriteWarnings =
    serde_json::from_reader(fs::File::open(overwrite_cache.join("warnings.json")).unwrap())
      .unwrap();
  assert_eq!(warnings, first_run.warnings);

  // A stream file an overwrite ignores is left out, so the second's entity 100 wins.
  write_overwrite(
    "resource_c/custom.ymap.diff.json",
    StreamFileOverwrite {
      schema_version: StreamFileOverwrite::SCHEMA_VERSION,
      ignore: true,
      source: None,
      vanilla: None,
      skip: None,
    },
  );
  // A merge reading an overwrite cache older than the overwrites fails.
  let stale = merge.run().unwrap_err().to_string();
  assert!(stale.contains("stale"), "{stale}");
  let ignored = merge.run_after_caches(&vanilla).unwrap();
  // The first's entity 150 is no longer placed as an entity of the versions before it.
  let model = merged();
  assert_eq!(guids(&model), [100, 200, 1200, 1250, 150, 0]);
  assert_eq!(model.entities[0].position.x, second.entities[0].position.x);
  assert_eq!(read(&output.join("ymap/solo.ymap")).entities[0].parent_index, 4);
  assert!(ignored.warnings.is_empty(), "{:?}", ignored.warnings);

  // A resource overwrite leaves an addition of the second's diff, against the empty YMAP, out.
  let second_document = document(&source_cache.join("diff/resource_b/custom.ymap.diff.json"));
  assert_eq!(second_document.vanilla, crate::core::source_cache::empty_baseline_file().unwrap());
  write_overwrite(
    "resource_b/custom.ymap.diff.json",
    StreamFileOverwrite {
      schema_version: StreamFileOverwrite::SCHEMA_VERSION,
      ignore: false,
      source: Some(OverwriteFingerprint {
        sha256: second_document.source.sha256,
        size: second_document.source.size,
      }),
      vanilla: Some(OverwriteFingerprint {
        sha256: second_document.vanilla.sha256,
        size: second_document.vanilla.size,
      }),
      skip: Some(serde_json::json!({"entities": [{"Added": {"entity": {"guid": 1200}}}]})),
    },
  );
  let applied = merge.run_after_caches(&vanilla).unwrap();
  assert!(applied.warnings.ignored_overwrites.is_empty(), "{:?}", applied.warnings);
  assert_eq!(guids(&merged()), [100, 200, 1250, 150, 0]);
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn added_ymaps_keep_parent_indices_into_added_parents() {
  let (bytes, catalog) = sample_bytes();
  let sample = sample_ymap("parent_refs/vanilla_parent.ymap");
  let root = std::env::temp_dir().join(format!("merge_added_parents_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let vanilla = root.join("vanilla");
  let output = root.join("merged");
  write_vanilla(&vanilla, &bytes);
  let resource = root.join("source/resource");
  fs::create_dir_all(resource.join("stream")).unwrap();
  fs::write(resource.join("fxmanifest.lua"), []).unwrap();
  // The child's entity has no GUID, so its parent stays an index into the added parent YMAP.
  let mut parent = sample.clone();
  parent.name = "added_lod".into();
  parent.entities[0].guid = 0;
  let mut child = sample.clone();
  child.name = "added_child".into();
  child.parent = "added_lod".into();
  child.entities.truncate(1);
  child.entities[0].guid = 0;
  child.entities[0].parent_index = 0;
  for (name, model) in [("added_lod.ymap", &parent), ("added_child.ymap", &child)] {
    fs::write(resource.join("stream").join(name), write_ymap(model, &model.entities).unwrap())
      .unwrap();
  }
  Merge {
    source_dir: root.join("source"),
    vanilla_cache_dir: root.join("vanilla-cache"),
    source_cache_dir: root.join("source-cache"),
    output_dir: output.clone(),
    overwrite_dir: None,
    overwrite_cache_dir: output.with_file_name("overwrite-cache"),
    force: false,
  }
  .run_after_caches(&vanilla)
  .unwrap();
  // Merged as they are, so both are deployed from the source cache.
  assert!(!output.join("ymap/added_lod.ymap").exists());
  assert!(!output.join("ymap/added_child.ymap").exists());
  let _ = catalog;
  fs::remove_dir_all(root).unwrap();
}
