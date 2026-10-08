use super::*;

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
  assert_eq!(crate::core::source_cache::load_merge_inputs(&output).unwrap().ybn.len(), 2);
  assert_eq!(
    crate::core::source_cache::load_moved_source_paths(&output).unwrap(),
    BTreeSet::from([
      "resource_a/stream/nested/collision.ybn".into(),
      "resource_b/streams/nested/collision.ybn".into(),
    ])
  );
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
    let old_id = crate::core::source_cache::inventory::resource_id(&source, &first).unwrap();
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
    let inputs = crate::core::source_cache::load_merge_inputs(&output).unwrap();
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
  let previous_id = crate::core::source_cache::inventory::resource_id(&source, &first).unwrap();
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
  assert_eq!(
    crate::core::source_cache::load_moved_source_paths(&output).unwrap(),
    BTreeSet::from([
      "[changed]/resource_a/stream/custom.ybn".into(),
      "[changed]/resource_a/stream/nested/collision.ybn".into(),
      "resource_b/streams/custom.ybn".into(),
      "resource_b/streams/nested/collision.ybn".into(),
    ])
  );
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
