use super::BuildSourceCache;
use super::{types::*, ymap_plan};
use crate::core::vanilla::{CacheVersion, VanillaCacheManifest, write_json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;

#[test]
fn builds_source_inventory_and_skips_unchanged_inputs_without_diffing() {
  let root = std::env::temp_dir().join(format!("source_cache_test_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let source = root.join("source");
  let source_resource = source.join("resource_a");
  let second_resource = source.join("resource_b");
  let vanilla = root.join("vanilla");
  let derived = root.join("vanilla-cache");
  let output = root.join("source-cache");
  fs::create_dir_all(source_resource.join("stream")).unwrap();
  fs::create_dir_all(second_resource.join("stream")).unwrap();
  fs::create_dir_all(vanilla.join("0000-base/ymap")).unwrap();
  fs::write(source_resource.join("fxmanifest.lua"), b"fx_version 'cerulean'").unwrap();
  fs::write(second_resource.join("fxmanifest.lua"), b"fx_version 'cerulean'").unwrap();
  fs::write(source_resource.join("stream/unmatched.ymap"), b"source bytes").unwrap();
  fs::write(second_resource.join("stream/unmatched.ymap"), b"other source bytes").unwrap();
  fs::write(source_resource.join("stream/unmatched.ybn"), b"source ybn").unwrap();
  fs::write(second_resource.join("stream/unmatched.ybn"), b"other source ybn").unwrap();
  let raw_manifest = VanillaCacheManifest {
    format_version: 1,
    game_dir: "game".into(),
    versions: vec![CacheVersion {
      id: "0000-base".into(),
      parent: None,
      archives: vec![],
      changes: BTreeMap::new(),
      unchanged: 0,
    }],
  };
  write_json(&vanilla.join("cache_info.json"), &raw_manifest).unwrap();

  let build = || BuildSourceCache {
    source_dir: source.clone(),
    output_dir: output.clone(),
    vanilla_dir: vanilla.clone(),
    vanilla_cache_dir: derived.clone(),
    force: false,
  };
  assert!(build().run().unwrap());
  assert!(output.join("source_cache_info.json").is_file());
  assert!(output.join("stream_conflicts.json").is_file());
  assert!(output.join("vanilla_ymaps_to_read.json").is_file());
  assert!(!output.join("diff_cache_info.json").exists());
  let info: SourceCacheMetadata =
    serde_json::from_reader(fs::File::open(output.join("source_cache_info.json")).unwrap())
      .unwrap();
  let resource_info = &info.resources["resource_a"];
  assert_eq!(resource_info.files_by_format[".ymap"].len(), 1);
  assert!(resource_info.files_by_format[".ymap"][0].vanilla.is_none());
  assert_eq!(info.resources.len(), 2);
  assert_eq!(info.conflict_count, 0);
  let report: SourceCacheConflictReport =
    serde_json::from_reader(fs::File::open(output.join("stream_conflicts.json")).unwrap()).unwrap();
  assert!(report.report.conflicts.is_empty());
  assert_eq!(report.source_conflicts.as_ref().unwrap().values().map(Vec::len).sum::<usize>(), 2);
  assert_eq!(info.format_version, 3);
  let merge_inputs = super::load_merge_inputs(&output).unwrap();
  assert!(merge_inputs.ymap.is_empty());
  assert!(merge_inputs.ybn.is_empty());
  let vanilla_ymaps_to_read: BTreeSet<String> =
    serde_json::from_reader(fs::File::open(output.join("vanilla_ymaps_to_read.json")).unwrap())
      .unwrap();
  assert!(vanilla_ymaps_to_read.is_empty());
  assert!(!build().run().unwrap());

  write_json(&output.join("stream_conflicts.json"), &report.report).unwrap();
  assert!(build().run().unwrap());
  assert!(!build().run().unwrap());
  for (name, file_name) in [("resource_a", "cached.ybn"), ("resource_b", "CACHED.YBN")] {
    fs::create_dir_all(output.join("resources").join(name).join("nested")).unwrap();
    fs::write(
      output.join("resources").join(name).join("nested").join(file_name),
      b"cache-only file",
    )
    .unwrap();
  }
  assert!(build().run().unwrap());
  let cache_report: SourceCacheConflictReport =
    serde_json::from_reader(fs::File::open(output.join("stream_conflicts.json")).unwrap()).unwrap();
  assert_eq!(cache_report.report.input_dir, output.to_string_lossy());
  assert_eq!(cache_report.report.scanned_file_count, 2);
  assert_eq!(cache_report.report.conflict_count, 1);
  assert_eq!(cache_report.report.conflicts[".ybn"][0].file_name, "cached.ybn");
  assert_eq!(
    cache_report.report.conflicts[".ybn"][0].paths,
    ["resources/resource_a/nested/cached.ybn", "resources/resource_b/nested/CACHED.YBN"]
  );
  assert_eq!(cache_report.source_conflicts, report.source_conflicts);
  fs::create_dir_all(output.join("_old/timestamp/resource_c")).unwrap();
  fs::write(output.join("_old/timestamp/resource_c/cached.ybn"), b"old bytes").unwrap();
  assert!(!build().run().unwrap());
  fs::remove_file(output.join("resources/resource_b/nested/CACHED.YBN")).unwrap();
  assert!(build().run().unwrap());
  let cache_report: SourceCacheConflictReport =
    serde_json::from_reader(fs::File::open(output.join("stream_conflicts.json")).unwrap()).unwrap();
  assert!(cache_report.report.conflicts.is_empty());
  assert_eq!(cache_report.report.scanned_file_count, 1);
  assert_eq!(cache_report.source_conflicts, report.source_conflicts);
  assert!(!build().run().unwrap());

  fs::remove_file(output.join("vanilla_ymaps_to_read.json")).unwrap();
  assert!(build().run().unwrap());

  let mut relationships: serde_json::Value =
    serde_json::from_reader(fs::File::open(derived.join("ymap_relationships.json")).unwrap())
      .unwrap();
  relationships["children_by_parent_hash"] = serde_json::json!({"00000001": ["child.ymap"]});
  write_json(&derived.join("ymap_relationships.json"), &relationships).unwrap();
  assert!(build().run().unwrap());
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn moves_vanilla_files_retains_history_and_updates_only_selected_resources() {
  use crate::core::vanilla::{CachedFile, FileChange};
  use sha2::{Digest, Sha256};

  let root = std::env::temp_dir().join(format!("source_moves_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let source = root.join("source");
  let mut first = source.join("resource_a");
  let second = source.join("resource_b");
  let empty = source.join("empty_resource");
  let vanilla = root.join("vanilla");
  let output = root.join("source-cache");
  for resource in [&first, &second, &empty] {
    fs::create_dir_all(resource).unwrap();
    fs::write(resource.join("fxmanifest.lua"), b"fx_version 'cerulean'").unwrap();
  }
  for (resource, stream) in [(&first, "stream"), (&second, "streams")] {
    fs::create_dir_all(resource.join(format!("{stream}/nested"))).unwrap();
    fs::write(resource.join(format!("{stream}/nested/collision.ybn")), b"original").unwrap();
    fs::write(resource.join(format!("{stream}/custom.ybn")), b"custom").unwrap();
  }
  fs::write(first.join("stream/custom.ymap"), b"unmatched ymap").unwrap();
  let object = "0000-base/ybn/collision.ybn";
  fs::create_dir_all(vanilla.join("0000-base/ybn")).unwrap();
  fs::write(vanilla.join(object), b"original").unwrap();
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
          "collision.ybn".into(),
          FileChange {
            previous_sha256: None,
            file: CachedFile {
              sha256: format!("{:x}", Sha256::digest(b"original")),
              object: object.into(),
              source: "base.rpf".into(),
            },
          },
        )]),
      }],
    },
  )
  .unwrap();
  let build = BuildSourceCache {
    source_dir: source.clone(),
    output_dir: output.clone(),
    vanilla_dir: vanilla,
    vanilla_cache_dir: root.join("vanilla-cache"),
    force: false,
  };
  assert!(build.run().unwrap());
  let read_metadata = || {
    serde_json::from_reader::<_, SourceCacheMetadata>(
      fs::File::open(output.join("source_cache_info.json")).unwrap(),
    )
    .unwrap()
  };
  let before = read_metadata();
  assert!(!first.join("stream/nested/collision.ybn").exists());
  assert!(first.join("stream/custom.ybn").is_file());
  assert_eq!(
    fs::read(output.join("resources/resource_a/nested/collision.ybn")).unwrap(),
    b"original"
  );
  assert!(before.resources["empty_resource"].files_by_format.is_empty());
  assert!(before.source_inputs.contains_key("empty_resource/"));
  assert!(before.source_inputs.contains_key("empty_resource/fxmanifest.lua"));
  assert!(!before.resource_checks["empty_resource"].has_stream);
  assert_eq!(before.conflict_count, 1);
  let report: serde_json::Value =
    serde_json::from_reader(fs::File::open(output.join("stream_conflicts.json")).unwrap()).unwrap();
  assert_eq!(
    report["conflicts"][".ybn"][0]["paths"],
    serde_json::json!([
      "resources/resource_a/nested/collision.ybn",
      "resources/resource_b/nested/collision.ybn",
    ])
  );
  assert_eq!(report["source_conflicts"][".ybn"].as_array().unwrap().len(), 1);
  assert_eq!(report["source_conflicts"][".ybn"][0]["fileName"], "custom.ybn");
  assert_eq!(
    report["source_conflicts"][".ybn"][0]["paths"],
    serde_json::json!(["resource_a/stream/custom.ybn", "resource_b/streams/custom.ybn",])
  );
  assert_eq!(super::load_merge_inputs(&output).unwrap().ybn.len(), 2);
  assert!(!build.run().unwrap());

  let mut legacy = read_metadata();
  for (name, stream) in [("resource_a", "stream"), ("resource_b", "streams")] {
    let original = output.join("resources").join(name).join("nested/collision.ybn");
    let old = if stream == "stream" {
      output.join(name).join(stream).join("nested/collision.ybn")
    } else {
      output.join(name).join("nested/collision.ybn")
    };
    fs::create_dir_all(old.parent().unwrap()).unwrap();
    fs::rename(&original, &old).unwrap();
    for file in legacy.resources.get_mut(name).unwrap().files_by_format.values_mut().flatten() {
      if file.cached_path.is_some() {
        file.cached_path =
          Some(old.strip_prefix(&output).unwrap().to_string_lossy().replace('\\', "/"));
      }
    }
  }
  write_json(&output.join("source_cache_info.json"), &legacy).unwrap();
  let modified = fs::metadata(output.join("resource_a/stream/nested/collision.ybn"))
    .unwrap()
    .modified()
    .unwrap();
  assert!(build.run_selected(Some("resource_a")).unwrap());
  assert_eq!(
    fs::metadata(output.join("resources/resource_a/nested/collision.ybn"))
      .unwrap()
      .modified()
      .unwrap(),
    modified
  );
  assert!(!output.join("resource_a/stream").exists());
  assert!(output.join("resource_b/nested/collision.ybn").is_file());
  assert!(!build.run_selected(Some("resource_a")).unwrap());
  assert!(build.run().unwrap());
  assert!(!output.join("resource_b").exists());
  assert_eq!(
    fs::read(output.join("resources/resource_b/nested/collision.ybn")).unwrap(),
    b"original"
  );
  assert_eq!(
    read_metadata().resources["resource_a"].files_by_format[".ybn"],
    before.resources["resource_a"].files_by_format[".ybn"]
  );
  assert!(!output.join("_old").exists());
  assert!(!build.run().unwrap());

  for (relative, selected, force) in [
    ("[qbx]/resource_a", None, false),
    ("[group]/resource_a", Some("resource_a"), false),
    ("resource_a", Some("resource_a"), true),
    ("[group]/resource_a", Some("[group]/resource_a"), false),
  ] {
    let old_id = super::inventory::resource_id(&source, &first).unwrap();
    let previous = read_metadata();
    let cached = output.join("resources/resource_a/nested/collision.ybn");
    let modified = fs::metadata(&cached).unwrap().modified().unwrap();
    let moved = source.join(relative);
    fs::create_dir_all(moved.parent().unwrap()).unwrap();
    fs::rename(&first, &moved).unwrap();
    let mut relocation = build.clone();
    relocation.force = force;
    assert!(relocation.run_selected(selected).unwrap());
    assert!(cached.is_file(), "Relocation must retain the cached stream file");
    assert_eq!(fs::read(&cached).unwrap(), b"original");
    assert_eq!(fs::metadata(&cached).unwrap().modified().unwrap(), modified);
    assert!(!output.join("_old").exists());
    let current = read_metadata();
    assert!(!current.resources.contains_key(&old_id));
    assert!(!current.resource_checks.contains_key(&old_id));
    assert!(!current.source_inputs.keys().any(|key| key.starts_with(&format!("{old_id}/"))));
    assert_eq!(current.resources.len(), previous.resources.len());
    assert_eq!(current.resources[relative].source, moved);
    assert!(current.ymap_load_plan.changed_source_parents.contains(&SourceYmapRef {
      resource: relative.into(),
      source_path: "stream/custom.ymap".into(),
      file_name: "custom.ymap".into(),
    }));
    assert!(
      !current.ymap_load_plan.changed_source_parents.iter().any(|source| source.resource == old_id)
    );
    assert_eq!(
      current.resources[relative].files_by_format,
      previous.resources[&old_id].files_by_format
    );
    if !force {
      assert_eq!(current.resource_checks[relative], previous.resource_checks[&old_id]);
    }
    for (key, fingerprint) in &previous.source_inputs {
      if let Some(suffix) = key.strip_prefix(&format!("{old_id}/")) {
        assert_eq!(&current.source_inputs[&format!("{relative}/{suffix}")], fingerprint);
      }
    }
    let inputs = super::load_merge_inputs(&output).unwrap();
    let input = inputs.ybn.iter().find(|file| file.resource == relative).unwrap();
    assert_eq!(input.path, cached);
    assert_eq!(input.original_path, moved.join("stream/nested/collision.ybn"));
    let conflicts: SourceCacheConflictReport =
      serde_json::from_reader(fs::File::open(output.join("stream_conflicts.json")).unwrap())
        .unwrap();
    assert!(
      conflicts.source_conflicts.as_ref().unwrap()[".ybn"]
        .iter()
        .any(|conflict| conflict.paths.contains(&format!("{relative}/stream/custom.ybn")))
    );
    assert!(conflicts.report.conflicts[".ybn"].iter().any(|conflict| {
      conflict.paths.contains(&"resources/resource_a/nested/collision.ybn".to_owned())
    }));
    assert!(!build.run_selected(selected).unwrap());
    assert!(!build.run().unwrap());
    first = moved;
  }
  let empty_before = read_metadata();
  let moved_empty = source.join("[group]/empty_resource");
  fs::rename(&empty, &moved_empty).unwrap();
  assert!(!build.run_selected(Some("resource_a")).unwrap());
  assert_eq!(read_metadata().resources["empty_resource"], empty_before.resources["empty_resource"]);
  assert!(build.run_selected(Some("[group]/empty_resource")).unwrap());
  let empty_after = read_metadata();
  assert!(!empty_after.resources.contains_key("empty_resource"));
  assert_eq!(empty_after.resources["[group]/empty_resource"].source, moved_empty);
  assert_eq!(
    empty_after.resource_checks["[group]/empty_resource"],
    empty_before.resource_checks["empty_resource"]
  );
  fs::rename(&moved_empty, &empty).unwrap();
  assert!(build.run().unwrap());
  assert!(!build.run().unwrap());
  assert!(!output.join("_old").exists());
  let before = read_metadata();

  fs::write(first.join("stream/nested/collision.ybn"), b"replacement").unwrap();
  fs::write(second.join("streams/nested/collision.ybn"), b"pending").unwrap();
  let previous_id = super::inventory::resource_id(&source, &first).unwrap();
  let moved = source.join("[changed]/resource_a");
  fs::create_dir_all(moved.parent().unwrap()).unwrap();
  fs::rename(&first, &moved).unwrap();
  first = moved;
  assert!(build.run_selected(Some("resource_a")).unwrap());
  assert_eq!(
    fs::read(output.join("resources/resource_a/nested/collision.ybn")).unwrap(),
    b"replacement"
  );
  assert_eq!(fs::read(second.join("streams/nested/collision.ybn")).unwrap(), b"pending");
  let after = read_metadata();
  assert!(!after.resources.contains_key(&previous_id));
  assert_eq!(after.resources["[changed]/resource_a"].source, first);
  assert_eq!(before.resources["resource_b"], after.resources["resource_b"]);
  assert_eq!(before.resource_checks["resource_b"], after.resource_checks["resource_b"]);
  let history = fs::read_dir(output.join("_old")).unwrap().next().unwrap().unwrap().path();
  assert_eq!(fs::read(history.join("resource_a/nested/collision.ybn")).unwrap(), b"original");
  let old: SourceCacheMetadata =
    serde_json::from_reader(fs::File::open(history.join("source_cache_info.json")).unwrap())
      .unwrap();
  assert_eq!(old, before);
  let old_file: FileFingerprint = serde_json::from_reader(
    fs::File::open(history.join("resource_a/nested/collision.ybn.fingerprint.json")).unwrap(),
  )
  .unwrap();
  assert_eq!(old_file.sha256, format!("{:x}", Sha256::digest(b"original")));
  assert!(!build.run_selected(Some("resource_a")).unwrap());
  let mut forced = build.clone();
  forced.force = true;
  assert!(forced.run_selected(Some("resource_a")).unwrap());
  assert_eq!(fs::read_dir(output.join("_old")).unwrap().count(), 1);
  assert!(second.join("streams/nested/collision.ybn").exists());

  let mut raw: VanillaCacheManifest =
    serde_json::from_reader(fs::File::open(build.vanilla_dir.join("cache_info.json")).unwrap())
      .unwrap();
  let object = "0000-base/ybn/custom.ybn";
  fs::write(build.vanilla_dir.join(object), b"custom").unwrap();
  raw.versions[0].changes.insert(
    "custom.ybn".into(),
    FileChange {
      previous_sha256: None,
      file: CachedFile {
        sha256: format!("{:x}", Sha256::digest(b"custom")),
        object: object.into(),
        source: "base.rpf".into(),
      },
    },
  );
  write_json(&build.vanilla_dir.join("cache_info.json"), &raw).unwrap();
  assert!(build.run_selected(Some("resource_a")).unwrap());
  assert!(!first.join("stream/custom.ybn").exists());
  assert!(output.join("resources/resource_a/custom.ybn").is_file());
  assert!(second.join("streams/custom.ybn").exists());
  assert!(build.run().unwrap());
  assert!(!second.join("streams/custom.ybn").exists());
  assert!(output.join("resources/resource_b/custom.ybn").is_file());
  assert!(!build.run().unwrap());

  fs::write(empty.join("fxmanifest.lua"), b"changed manifest").unwrap();
  assert!(build.run_selected(Some("empty_resource")).unwrap());
  fs::create_dir_all(empty.join("stream")).unwrap();
  assert!(build.run_selected(Some("empty_resource")).unwrap());
  assert!(read_metadata().resource_checks["empty_resource"].has_stream);
  assert!(!build.run_selected(Some("empty_resource")).unwrap());
  fs::remove_dir_all(empty.join("stream")).unwrap();
  assert!(build.run_selected(Some("empty_resource")).unwrap());
  assert!(build.run_selected(Some("missing")).is_err());
  assert!(!build.run().unwrap());
  fs::remove_dir_all(&second).unwrap();
  assert!(build.run().unwrap());
  assert!(!read_metadata().resources.contains_key("resource_b"));
  assert!(!output.join("resources/resource_b").exists());
  assert!(!build.run().unwrap());
  let previous = read_metadata();
  let mut ambiguous = previous.clone();
  let mut duplicate = ambiguous.resources["[changed]/resource_a"].clone();
  duplicate.source = source.join("[duplicate]/resource_a");
  ambiguous.resources.insert("[duplicate]/resource_a".into(), duplicate);
  write_json(&output.join("source_cache_info.json"), &ambiguous).unwrap();
  let history_count = fs::read_dir(output.join("_old")).unwrap().count();
  let error = build.run_selected(Some("resource_a")).unwrap_err();
  assert!(error.to_string().contains("Ambiguous cached resource name"));
  assert!(output.join("resources/resource_a/nested/collision.ybn").is_file());
  assert_eq!(fs::read_dir(output.join("_old")).unwrap().count(), history_count);
  write_json(&output.join("source_cache_info.json"), &previous).unwrap();
  let duplicate = source.join("[duplicate]/resource_a");
  fs::create_dir_all(&duplicate).unwrap();
  fs::write(duplicate.join("fxmanifest.lua"), []).unwrap();
  assert!(build.run().unwrap_err().to_string().contains("Duplicate or reserved resource name"));
  assert!(output.join("resources/resource_a/nested/collision.ybn").is_file());
  fs::remove_dir_all(duplicate.parent().unwrap()).unwrap();
  fs::create_dir_all(first.join("streams/nested")).unwrap();
  fs::write(first.join("streams/nested/collision.ybn"), b"conflicting streams").unwrap();
  assert!(
    build
      .run_selected(Some("resource_a"))
      .unwrap_err()
      .to_string()
      .contains("Conflicting stream-relative cache paths")
  );
  assert_eq!(
    fs::read(output.join("resources/resource_a/nested/collision.ybn")).unwrap(),
    b"replacement"
  );
  assert!(first.join("streams/nested/collision.ybn").is_file());
  assert_eq!(fs::read_dir(output.join("_old")).unwrap().count(), history_count);
  fs::remove_dir_all(first.join("streams")).unwrap();
  fs::remove_file(output.join("resources/resource_a/nested/collision.ybn")).unwrap();
  assert!(build.run_selected(Some("resource_a")).is_err());
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn source_cache_content_freshness_ignores_timestamp_only_changes() {
  let fingerprint = FileFingerprint {
    sha256: "same-content".into(),
    modified_seconds: 10,
    modified_nanos: 20,
    size: 30,
  };
  let cached = BTreeMap::from([("resource/stream/map.ymap".into(), fingerprint.clone())]);
  let mut touched = fingerprint;
  touched.modified_seconds += 1;
  touched.modified_nanos += 1;
  let current = BTreeMap::from([("resource/stream/map.ymap".into(), touched)]);

  assert!(super::publication::same_source_content(&cached, &current));
}

#[test]
fn source_cache_timestamp_refresh_updates_provenance_without_regeneration() {
  let root = std::env::temp_dir().join(format!("source_timestamp_refresh_{}", std::process::id()));
  fs::create_dir_all(&root).unwrap();
  let cached = FileFingerprint {
    sha256: "same-content".into(),
    modified_seconds: 10,
    modified_nanos: 20,
    size: 30,
  };
  let mut current = cached.clone();
  current.modified_seconds += 1;
  current.modified_nanos += 1;
  write_json(
    &root.join("source_cache_info.json"),
    &SourceCacheMetadata {
      format_version: 2,
      source_dir: root.join("source"),
      vanilla_dir: root.join("vanilla"),
      vanilla_cache_dir: root.join("vanilla-cache"),
      vanilla_manifest_sha256: "manifest".into(),
      vanilla_cache_revision: "derived".into(),
      latest_vanilla_version: "latest".into(),
      generated_at: "original-generation".into(),
      source_inputs: BTreeMap::from([("resource_a/stream/map.ymap".into(), cached)]),
      resource_checks: BTreeMap::new(),
      resources: BTreeMap::from([(
        "resource_a".into(),
        ResourceInventory {
          source: root.join("source/resource_a"),
          files_by_format: BTreeMap::from([(
            ".ymap".into(),
            vec![SourceFile {
              path: "stream/map.ymap".into(),
              file_name: "map.ymap".into(),
              sha256: "same-content".into(),
              size: 30,
              modified_seconds: 10,
              modified_nanos: 20,
              vanilla: None,
              ymap_parent_hash: None,
              metadata_error: None,
              cached_path: None,
            }],
          )]),
        },
      )]),
      scanned_stream_file_count: 1,
      conflict_count: 0,
      ymap_load_plan: YmapLoadPlan {
        changed_source_parents: BTreeSet::new(),
        additional_source_children: BTreeSet::new(),
      },
      outputs: BTreeSet::new(),
    },
  )
  .unwrap();

  super::publication::refresh_source_timestamps(
    &root,
    &BTreeMap::from([("resource_a/stream/map.ymap".into(), current)]),
  )
  .unwrap();

  let refreshed: SourceCacheMetadata =
    serde_json::from_reader(fs::File::open(root.join("source_cache_info.json")).unwrap()).unwrap();
  assert_eq!(refreshed.generated_at, "original-generation");
  assert_eq!(refreshed.source_inputs["resource_a/stream/map.ymap"].modified_seconds, 11);
  assert_eq!(refreshed.resources["resource_a"].files_by_format[".ymap"][0].modified_seconds, 11);
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn merge_input_selector_uses_format_conflicts_and_ymap_load_closure() {
  use crate::core::stream_conflicts::{StreamConflictReport, StreamFileConflict};

  let root = std::env::temp_dir().join(format!("source_merge_inputs_{}", std::process::id()));
  let source_dir = root.join("source");
  let cache_dir = root.join("source-cache");
  let resource_a = source_dir.join("resource_a");
  let resource_b = source_dir.join("resource_b");
  fs::create_dir_all(resource_a.join("stream")).unwrap();
  fs::create_dir_all(resource_b.join("stream")).unwrap();
  for path in [
    resource_a.join("stream/parent.ymap"),
    resource_a.join("stream/unmatched.ymap"),
    resource_a.join("stream/collision.ybn"),
    resource_a.join("stream/unique.ybn"),
    resource_b.join("stream/child.ymap"),
    resource_b.join("stream/collision.ybn"),
  ] {
    fs::write(path, []).unwrap();
  }
  fs::create_dir_all(&cache_dir).unwrap();
  let source_dir = source_dir.canonicalize().unwrap();
  let resource_file = |path: &str, file_name: &str, vanilla: Option<VanillaMatch>| SourceFile {
    path: path.into(),
    file_name: file_name.into(),
    sha256: "source-hash".into(),
    size: 1,
    modified_seconds: 1,
    modified_nanos: 0,
    vanilla,
    ymap_parent_hash: None,
    metadata_error: None,
    cached_path: None,
  };
  let matched = |content_matches| {
    Some(VanillaMatch {
      version: "0000-base".into(),
      sha256: "vanilla-hash".into(),
      content_matches,
    })
  };
  let source_ref = |resource: &str, source_path: &str, file_name: &str| SourceYmapRef {
    resource: resource.into(),
    source_path: source_path.into(),
    file_name: file_name.into(),
  };
  write_json(
    &cache_dir.join("source_cache_info.json"),
    &SourceCacheMetadata {
      format_version: 2,
      source_dir: source_dir.clone(),
      vanilla_dir: root.join("vanilla"),
      vanilla_cache_dir: root.join("vanilla-cache"),
      vanilla_manifest_sha256: "manifest".into(),
      vanilla_cache_revision: "derived".into(),
      latest_vanilla_version: "0000-base".into(),
      generated_at: "now".into(),
      source_inputs: BTreeMap::new(),
      resource_checks: BTreeMap::new(),
      resources: BTreeMap::from([
        (
          "resource_a".into(),
          ResourceInventory {
            source: source_dir.join("resource_a"),
            files_by_format: BTreeMap::from([
              (
                ".ymap".into(),
                vec![
                  resource_file("stream/parent.ymap", "parent.ymap", matched(false)),
                  resource_file("stream/unmatched.ymap", "unmatched.ymap", None),
                ],
              ),
              (
                ".ybn".into(),
                vec![
                  resource_file("stream/collision.ybn", "collision.ybn", matched(true)),
                  resource_file("stream/unique.ybn", "unique.ybn", matched(false)),
                ],
              ),
            ]),
          },
        ),
        (
          "resource_b".into(),
          ResourceInventory {
            source: source_dir.join("resource_b"),
            files_by_format: BTreeMap::from([
              (
                ".ymap".into(),
                vec![resource_file("stream/child.ymap", "child.ymap", matched(true))],
              ),
              (
                ".ybn".into(),
                vec![resource_file("stream/collision.ybn", "collision.ybn", matched(true))],
              ),
            ]),
          },
        ),
      ]),
      scanned_stream_file_count: 6,
      conflict_count: 1,
      ymap_load_plan: YmapLoadPlan {
        changed_source_parents: BTreeSet::from([source_ref(
          "resource_a",
          "stream/parent.ymap",
          "parent.ymap",
        )]),
        additional_source_children: BTreeSet::from([source_ref(
          "resource_b",
          "stream/child.ymap",
          "child.ymap",
        )]),
      },
      outputs: BTreeSet::new(),
    },
  )
  .unwrap();
  write_json(
    &cache_dir.join("stream_conflicts.json"),
    &StreamConflictReport {
      input_dir: source_dir.to_string_lossy().into_owned(),
      scanned_file_count: 6,
      conflict_count: 1,
      conflicts: BTreeMap::from([(
        ".ybn".into(),
        vec![StreamFileConflict {
          file_name: "collision.ybn".into(),
          paths: vec![
            "resource_a/stream/collision.ybn".into(),
            "resource_b/stream/collision.ybn".into(),
          ],
        }],
      )]),
    },
  )
  .unwrap();
  write_json(
    &cache_dir.join("vanilla_ymaps_to_read.json"),
    &BTreeSet::from(["parent.ymap".to_owned(), "child.ymap".to_owned()]),
  )
  .unwrap();

  let inputs = super::load_merge_inputs(&cache_dir).unwrap();
  assert_eq!(
    inputs.ymap.iter().map(|source| source.file_name.as_str()).collect::<Vec<_>>(),
    ["parent.ymap", "child.ymap"]
  );
  assert_eq!(inputs.ybn.len(), 2);
  assert!(inputs.ybn.iter().all(|source| source.file_name == "collision.ybn"));
  assert_eq!(
    inputs.vanilla_ymaps_to_read,
    BTreeSet::from(["parent.ymap".into(), "child.ymap".into()])
  );
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn parent_change_lists_vanilla_closure_and_source_children_in_other_resources() {
  let parent_name = "parent_map.ymap";
  let child_name = "child_map.ymap";
  let parent_hash = crate::core::format::ymap::diff::reference_hash("parent_map");
  let source_file =
    |file_name: &str, vanilla: Option<VanillaMatch>, parent: Option<String>| SourceFile {
      path: format!("stream/{file_name}"),
      file_name: file_name.into(),
      sha256: "source-hash".into(),
      size: 10,
      modified_seconds: 1,
      modified_nanos: 0,
      vanilla,
      ymap_parent_hash: parent,
      metadata_error: None,
      cached_path: None,
    };
  let resources = BTreeMap::from([
    (
      "resource_parent".into(),
      ResourceInventory {
        source: "resource_parent".into(),
        files_by_format: BTreeMap::from([(
          ".ymap".into(),
          vec![source_file(
            parent_name,
            Some(VanillaMatch {
              version: "0000-base".into(),
              sha256: "old-parent-hash".into(),
              content_matches: false,
            }),
            None,
          )],
        )]),
      },
    ),
    (
      "resource_child".into(),
      ResourceInventory {
        source: "resource_child".into(),
        files_by_format: BTreeMap::from([(
          ".ymap".into(),
          vec![source_file(
            child_name,
            Some(VanillaMatch {
              version: "0000-base".into(),
              sha256: "vanilla-hash".into(),
              content_matches: true,
            }),
            Some(format!("{parent_hash:08X}")),
          )],
        )]),
      },
    ),
  ]);
  let relationships = YmapRelationshipIndex {
    format_version: 2,
    vanilla_manifest_sha256: "revision".into(),
    version: "latest".into(),
    children_by_parent_hash: BTreeMap::from([(
      format!("{parent_hash:08x}"),
      vec![child_name.into(), "vanilla_only_child.ymap".into()],
    )]),
  };
  let vanilla_files = BTreeMap::from([
    (
      parent_name.into(),
      DerivedVanillaFile {
        version: "0000-base".into(),
        sha256: "parent".into(),
        object: "latest/ymap/parent_map.ymap".into(),
      },
    ),
    (
      child_name.into(),
      DerivedVanillaFile {
        version: "0000-base".into(),
        sha256: "child".into(),
        object: "latest/ymap/child_map.ymap".into(),
      },
    ),
    (
      "vanilla_only_child.ymap".into(),
      DerivedVanillaFile {
        version: "0000-base".into(),
        sha256: "vanilla-child".into(),
        object: "latest/ymap/vanilla_only_child.ymap".into(),
      },
    ),
  ]);

  let plan = ymap_plan::build(&resources);
  assert!(plan.changed_source_parents.contains(&SourceYmapRef {
    resource: "resource_parent".into(),
    source_path: "stream/parent_map.ymap".into(),
    file_name: parent_name.into(),
  }));
  assert!(plan.additional_source_children.contains(&SourceYmapRef {
    resource: "resource_child".into(),
    source_path: "stream/child_map.ymap".into(),
    file_name: child_name.into(),
  }));
  let vanilla_ymaps_to_read =
    ymap_plan::vanilla_ymaps_to_read(&plan.changed_source_parents, &relationships, &vanilla_files);
  assert!(vanilla_ymaps_to_read.contains(parent_name));
  assert!(vanilla_ymaps_to_read.contains(child_name));
  assert!(vanilla_ymaps_to_read.contains("vanilla_only_child.ymap"));
}
