use super::*;
use crate::core::format::gamefile::meta::{MetaResource, MetaSchemaCatalog};
use crate::core::format::gamefile::rsc7::Rsc7Resource;
use crate::core::format::gamefile::test_support::sample_ymap;
use crate::core::format::ymap::binary::{write_ymap, ymap_to_model};
use crate::core::format::ymap::compare::YmapComparisonDiff;
use crate::core::format::ymap::model::ymap::Ymap;
use crate::core::format::ymap::reference_hash;
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
  fs::write(resource.join("stream/maps/map.ymap"), write_ymap(&edited, &edited.entities).unwrap())
    .unwrap();

  let merge = Merge {
    source_dir: root.join("source"),
    vanilla_cache_dir: root.join("vanilla-cache"),
    source_cache_dir: source_cache.clone(),
    output_dir: output.clone(),
    overwrite_dir: Some(overwrite_dir.clone()),
    overwrite_cache_dir: Some(root.join("overwrite-cache")),
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
  assert!(merge.run_after_caches(&vanilla).unwrap().ignored_overwrites.is_empty());
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
  assert_eq!(merge.run_after_caches(&vanilla).unwrap().ignored_overwrites, stale);
  assert_eq!(merged().content_flags, edited.content_flags);
  let reused = merge.run_after_caches(&vanilla).unwrap();
  assert!(reused.changes.added.is_empty() && reused.changes.modified.is_empty());
  assert_eq!(reused.ignored_overwrites, stale);

  // One naming a stream file the resource does not merge is reported too.
  fs::create_dir_all(overwrite_dir.join("resources/resource/maps")).unwrap();
  fs::rename(&overwrite_path, overwrite_dir.join("resources/resource/maps/other.ymap.diff.json"))
    .unwrap();
  assert_eq!(
    merge.run_after_caches(&vanilla).unwrap().ignored_overwrites,
    vec![IgnoredOverwrite {
      resource: Some("resource".into()),
      file: "maps/other.ymap".into(),
      reason: IgnoredOverwriteReason::MissingSource,
    }]
  );
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
    overwrite_cache_dir: Some(root.join("overwrite-cache")),
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
    merge.run_after_caches(&vanilla).unwrap().ignored_overwrites,
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
fn added_ymaps_merge_from_their_base_sources() {
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
  // Three resources add `custom.ymap`: the second with other entities and extents, the third
  // with one entity more than the first. The first also adds `solo.ymap`, a child of it.
  let mut first = sample.clone();
  first.name = "custom".into();
  let mut second = first.clone();
  for entity in &mut second.entities {
    entity.guid += 1000;
    entity.position.x += 50.0;
  }
  second.streaming_extents_max.x += 50.0;
  let mut third = first.clone();
  let mut extra = third.entities[1].clone();
  extra.guid = 1300;
  extra.position.y += 20.0;
  third.entities.push(extra);
  let mut solo = sample.clone();
  solo.name = "solo".into();
  solo.parent = "custom".into();
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
    overwrite_cache_dir: Some(overwrite_cache.clone()),
    force: false,
  };
  let read = |path: &Path| ymap_to_model(&fs::read(path).unwrap(), &catalog.hash_names).unwrap();
  let guids = |model: &Ymap| model.entities.iter().map(|entity| entity.guid).collect::<Vec<_>>();
  let merged = || guids(&read(&output.join("ymap/custom.ymap")));
  let added_ymaps = || -> serde_json::Value {
    serde_json::from_reader(fs::File::open(overwrite_cache.join("added_ymaps.json")).unwrap())
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
  let skip = |document: YmapComparisonDiff, guid: u32| StreamFileOverwrite {
    schema_version: StreamFileOverwrite::SCHEMA_VERSION,
    ignore: false,
    source: Some(OverwriteFingerprint {
      sha256: document.source.sha256,
      size: document.source.size,
    }),
    vanilla: Some(OverwriteFingerprint {
      sha256: document.vanilla.sha256,
      size: document.vanilla.size,
    }),
    skip: Some(serde_json::json!({"entities": [{"Added": {"entity": {"guid": guid}}}]})),
  };
  let ignore = StreamFileOverwrite {
    schema_version: StreamFileOverwrite::SCHEMA_VERSION,
    ignore: true,
    source: None,
    vanilla: None,
    skip: None,
  };
  merge.run_after_caches(&vanilla).unwrap();

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

  // Of the three `custom.ymap`, that of the resource loaded last wins and is written, so that it
  // replaces the others; `solo.ymap`, which the merge leaves as it is, is deployed from the
  // source cache.
  assert_eq!(merged(), [100, 200, 1300]);
  assert!(!output.join("ymap/solo.ymap").exists());
  let winners = added_ymaps();
  assert_eq!(winners["files"]["custom.ymap"]["base"]["resource"], "resource_c");
  assert_eq!(winners["files"]["custom.ymap"]["chosen_by"], "last_loaded");
  assert_eq!(winners["files"]["solo.ymap"]["chosen_by"], "only");
  assert_eq!(
    winners["files"]["custom.ymap"]["shipped_by"],
    serde_json::json!(["resource_a", "resource_b", "resource_c"])
  );

  // A stream file an overwrite ignores is left out of the choice.
  write_overwrite("resource_c/custom.ymap.diff.json", ignore.clone());
  merge.run_after_caches(&vanilla).unwrap();
  assert_eq!(merged(), [1100, 1200]);

  // A base source takes the others' differences from it, which the overwrite cache holds; the
  // ignored one is left out.
  fs::create_dir_all(overwrite_dir.join("added/ymap")).unwrap();
  write_json(
    &overwrite_dir.join("added/ymap/Custom.ymap.json"),
    &AddedYmapOverwrite {
      schema_version: AddedYmapOverwrite::SCHEMA_VERSION,
      base_source: "resource_a".into(),
    },
  )
  .unwrap();
  // A merge reading an overwrite cache older than the overwrites fails.
  let stale = merge.run().unwrap_err().to_string();
  assert!(stale.contains("stale"), "{stale}");
  fs::remove_file(overwrite_dir.join("resources/resource_c/custom.ymap.diff.json")).unwrap();
  write_overwrite("resource_b/custom.ymap.diff.json", ignore);
  merge.run_after_caches(&vanilla).unwrap();
  assert_eq!(merged(), [100, 200, 1300]);
  let winners = added_ymaps();
  assert_eq!(winners["files"]["custom.ymap"]["base"]["resource"], "resource_a");
  assert_eq!(winners["files"]["custom.ymap"]["chosen_by"], "overwrite");
  assert_eq!(
    winners["files"]["custom.ymap"]["sources"],
    serde_json::json!([{
      "resource": "resource_c",
      "stream_path": "custom.ymap",
      "diff": "diff/resource_c/custom.ymap.diff.json"
    }])
  );
  let other_path = overwrite_cache.join("diff/resource_c/custom.ymap.diff.json");
  assert_eq!(document(&other_path).vanilla.path, "resources/resource_a/custom.ymap");

  // Overwrites leave out an entity of the base source, against the empty YMAP, and the other
  // source's entity, against the base source.
  write_overwrite(
    "resource_a/custom.ymap.diff.json",
    skip(document(&source_cache.join("diff/resource_a/custom.ymap.diff.json")), 100),
  );
  write_overwrite("resource_a/solo.ymap.diff.json", skip(solo_document, 200));
  assert!(merge.run_after_caches(&vanilla).unwrap().ignored_overwrites.is_empty());
  assert_eq!(merged(), [200, 1300]);
  assert!(overwrite_cache.join("base/resource_a/custom.ymap.diff.json").is_file());
  let solo_merged = read(&output.join("ymap/solo.ymap"));
  assert_eq!(guids(&solo_merged), [100]);
  assert_eq!(reference_hash(&solo_merged.parent), reference_hash("custom"));
  write_overwrite("resource_c/custom.ymap.diff.json", skip(document(&other_path), 1300));
  assert!(merge.run_after_caches(&vanilla).unwrap().ignored_overwrites.is_empty());
  assert_eq!(merged(), [200]);

  // Without the base source, the resource loaded last wins again: the first's overwrite names no
  // merged source, and the third's was written against another baseline.
  fs::remove_file(overwrite_dir.join("added/ymap/Custom.ymap.json")).unwrap();
  let ignored = merge.run_after_caches(&vanilla).unwrap().ignored_overwrites;
  assert_eq!(merged(), [100, 200, 1300]);
  assert_eq!(
    ignored,
    vec![
      IgnoredOverwrite {
        resource: Some("resource_a".into()),
        file: "custom.ymap".into(),
        reason: IgnoredOverwriteReason::MissingSource,
      },
      IgnoredOverwrite {
        resource: Some("resource_c".into()),
        file: "custom.ymap".into(),
        reason: IgnoredOverwriteReason::VanillaChanged,
      },
    ]
  );
  assert!(!other_path.exists());
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
    overwrite_cache_dir: None,
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
