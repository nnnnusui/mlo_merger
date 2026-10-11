use super::*;
use std::collections::BTreeMap;

#[path = "test_fixtures.rs"]
mod fixtures;
use fixtures::{fixture, native_catalog, native_resources};

#[path = "overwrite_cache_tests.rs"]
mod overwrite_cache_tests;

#[test]
fn same_direction_and_hash_keep_distinct_occurrences_and_remove_only_selected_pair() {
  let (mut maps, fingerprints) = fixture();
  let export = snapshot::document("resource", "child.ymap", &maps, &fingerprints).unwrap();
  assert_eq!(export.lights.len(), 3);
  assert!(export.lights.iter().all(|light| light.editable));
  let edit = YmapLightEditDocument {
    format: "mlo_lod_light_edits".into(),
    schema_version: 1,
    resource: export.resource.clone(),
    path: export.path.clone(),
    files: export.files.clone(),
    operations: vec![LightOperation::Remove {
      light_id: export.lights[1].id.clone(),
    }],
  };
  edits::apply(&export, &edit, &mut maps).unwrap();
  assert_eq!(maps["child.ymap"].model.lod_lights.len(), 2);
  let parent = &maps["parent.ymap"].model.distant_lod_lights;
  assert_eq!(
    parent.items.iter().map(|light| light.position.as_str()).collect::<Vec<_>>(),
    ["0 0 0", "2 0 0"]
  );
  assert_eq!(parent.num_street_lights, 2);
}

#[test]
fn stale_unknown_or_unresolved_removals_fail_without_mutation() {
  let (mut maps, fingerprints) = fixture();
  let export = snapshot::document("resource", "child.ymap", &maps, &fingerprints).unwrap();
  let before = maps["child.ymap"].model.clone();
  let mut edit = YmapLightEditDocument {
    format: "mlo_lod_light_edits".into(),
    schema_version: 1,
    resource: export.resource.clone(),
    path: export.path.clone(),
    files: export.files.clone(),
    operations: vec![LightOperation::Remove {
      light_id: "unknown".into(),
    }],
  };
  assert!(edits::apply(&export, &edit, &mut maps).is_err());
  edit.operations.clear();
  edit.files[0].sha256 = "stale".into();
  assert!(edits::apply(&export, &edit, &mut maps).is_err());
  assert_eq!(maps["child.ymap"].model, before);
  maps.remove("parent.ymap");
  let orphan = snapshot::document("resource", "child.ymap", &maps, &fingerprints).unwrap();
  assert!(!orphan.diagnostics.is_empty());
  assert!(orphan.lights.is_empty());
}

#[test]
fn native_export_reuses_repairs_and_rejects_stale_parent_snapshot() {
  use std::fs;
  let root = std::env::temp_dir().join(format!("light_export_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let source = native_resources(&root, &["resource"]);
  let command = ExportLights {
    input: source.join("resource/stream/nested/child.ymap"),
    resource: "resource".into(),
    source_dir: source,
    source_cache_dir: root.join("source-cache"),
    output_dir: root.join("json"),
    force: false,
  };
  let first = command.run().unwrap();
  let read = || {
    serde_json::from_reader::<_, YmapLightDocument>(fs::File::open(&first.path).unwrap()).unwrap()
  };
  let export = read();
  assert_eq!(export.path, "nested/child.ymap");
  assert_eq!(export.lights.len(), 3);
  let time = fs::metadata(&first.path).unwrap().modified().unwrap();
  assert!(command.run().unwrap().skipped);
  assert_eq!(fs::metadata(&first.path).unwrap().modified().unwrap(), time);
  let mut unrelated = read_map(&resource::ResourceFile {
    path: command.input.clone(),
    original: command.input.clone(),
    parent_hint: None,
  })
  .unwrap();
  unrelated.model.lod_lights.clear();
  unrelated.model.name = "unrelated".into();
  fs::write(
    command.input.parent().unwrap().join("unrelated.ymap"),
    crate::core::format::ymap::binary::write_ymap(&unrelated.model, &[], &native_catalog())
      .unwrap(),
  )
  .unwrap();
  assert!(command.run().unwrap().skipped);
  assert_eq!(fs::metadata(&first.path).unwrap().modified().unwrap(), time);
  let parent = command.source_dir.join("resource/stream/nested/parent.ymap");
  fs::OpenOptions::new()
    .write(true)
    .open(&parent)
    .unwrap()
    .set_times(
      fs::FileTimes::new()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5)),
    )
    .unwrap();
  assert!(command.run().unwrap().skipped);
  assert_eq!(fs::metadata(&first.path).unwrap().modified().unwrap(), time);
  fs::write(&first.path, b"broken").unwrap();
  assert!(!command.run().unwrap().skipped);
  assert_eq!(read(), export);
  let edit = YmapLightEditDocument {
    format: "mlo_lod_light_edits".into(),
    schema_version: 1,
    resource: export.resource.clone(),
    path: export.path.clone(),
    files: export.files.clone(),
    operations: vec![LightOperation::Remove {
      light_id: export.lights[0].id.clone(),
    }],
  };
  let overwrite = root.join("overwrite");
  fs::create_dir_all(overwrite.join("resource/nested")).unwrap();
  crate::core::vanilla::write_json(&overwrite.join("resource/nested/child.ymap.diff.json"), &edit)
    .unwrap();
  assert!(PreparedEdits::load(&command.source_dir, &command.source_cache_dir, &overwrite).is_ok());
  let input = resource::ResourceFile {
    path: parent.clone(),
    original: parent.clone(),
    parent_hint: None,
  };
  let mut changed = read_map(&input).unwrap();
  changed.model.distant_lod_lights.items[0].position = "99 0 0".into();
  fs::write(
    &parent,
    crate::core::format::ymap::binary::write_ymap(
      &changed.model,
      &changed.entities,
      &native_catalog(),
    )
    .unwrap(),
  )
  .unwrap();
  assert!(PreparedEdits::load(&command.source_dir, &command.source_cache_dir, &overwrite).is_err());
  assert!(!command.run().unwrap().skipped);
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn native_custom_light_edits_merge_deploy_reuse_and_retract() {
  use crate::core::vanilla::{
    CacheVersion, CachedFile, FileChange, VanillaCacheManifest, write_json,
  };
  use sha2::{Digest, Sha256};
  use std::fs;
  let root = std::env::temp_dir().join(format!("light_merge_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let source = native_resources(&root, &["a", "b"]);
  let vanilla = root.join("vanilla");
  let cache = root.join("vanilla-cache");
  let source_cache = root.join("source-cache");
  let output = root.join("merged");
  let overwrite = root.join("overwrite");
  let bytes = fs::read(source.join("a/stream/nested/child.ymap")).unwrap();
  let object = "0000-base/ymap/unrelated.ymap";
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
          "unrelated.ymap".into(),
          FileChange {
            previous_sha256: None,
            file: CachedFile {
              sha256: format!("{:x}", Sha256::digest(&bytes)),
              object: object.into(),
              source: "fixture.rpf".into(),
            },
          },
        )]),
      }],
    },
  )
  .unwrap();
  let export_result = ExportLights {
    input: source.join("a/stream/nested/child.ymap"),
    resource: "a".into(),
    source_dir: source.clone(),
    source_cache_dir: source_cache.clone(),
    output_dir: root.join("json"),
    force: false,
  }
  .run()
  .unwrap();
  let export: YmapLightDocument =
    serde_json::from_reader(fs::File::open(export_result.path).unwrap()).unwrap();
  let edit = YmapLightEditDocument {
    format: "mlo_lod_light_edits".into(),
    schema_version: 1,
    resource: "a".into(),
    path: export.path.clone(),
    files: export.files.clone(),
    operations: vec![LightOperation::Remove {
      light_id: export.lights[1].id.clone(),
    }],
  };
  fs::create_dir_all(overwrite.join("a/nested")).unwrap();
  let diff = overwrite.join("a/nested/child.ymap.diff.json");
  write_json(&diff, &edit).unwrap();
  let run = || {
    crate::core::merge::merge::run_with_overwrite(
      &source,
      &vanilla,
      &cache,
      &source_cache,
      &output,
      false,
      &overwrite,
    )
    .unwrap()
  };
  run();
  let read_output =
    |name: &str| crate::core::vanilla::read_ymap(&output.join("ymap").join(name)).unwrap();
  assert_eq!(read_output("parent.ymap").distant_lod_lights.items.len(), 3);
  assert_eq!(read_output("child.ymap").lod_lights.len(), 3);
  let before = fs::metadata(output.join("ymap/parent.ymap")).unwrap().modified().unwrap();
  run();
  assert_eq!(fs::metadata(output.join("ymap/parent.ymap")).unwrap().modified().unwrap(), before);
  fs::remove_dir_all(source.join("b")).unwrap();
  run();
  assert_eq!(read_output("parent.ymap").distant_lod_lights.items.len(), 2);
  assert_eq!(read_output("child.ymap").lod_lights.len(), 2);
  assert_eq!(read_output("parent.ymap").distant_lod_lights.num_street_lights, 2);
  let deploy = crate::core::deploy::Deploy {
    merged_dir: output.clone(),
    source_cache_dir: source_cache.clone(),
    output_dir: root.join("deploy"),
    force: false,
  };
  deploy.run().unwrap();
  assert!(
    fs::read_to_string(root.join("deploy/files.txt"))
      .unwrap()
      .contains("a/stream/nested/parent.ymap")
  );
  fs::remove_file(diff).unwrap();
  run();
  assert!(!output.join("ymap/parent.ymap").exists());
  assert!(!output.join("light_edit_provenance.json").exists());
  deploy.run().unwrap();
  assert_eq!(fs::read_to_string(root.join("deploy/files.txt")).unwrap(), "");
  assert_eq!(
    crate::core::vanilla::read_ymap(&source.join("a/stream/nested/parent.ymap"))
      .unwrap()
      .distant_lod_lights
      .items
      .len(),
    3
  );
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn shared_children_and_export_bounds_follow_grouped_resource_identity() {
  use std::fs;
  let root = std::env::temp_dir().join(format!("light_shared_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let source = native_resources(&root, &["[group]/resource"]);
  let input = source.join("[group]/resource/stream/nested/child.ymap");
  let mut sibling = read_map(&resource::ResourceFile {
    path: input.clone(),
    original: input.clone(),
    parent_hint: None,
  })
  .unwrap();
  sibling.model.name = "second_child".into();
  let stream = input.parent().unwrap();
  fs::write(
    stream.join("second_child.ymap"),
    crate::core::format::ymap::binary::write_ymap(&sibling.model, &[], &native_catalog()).unwrap(),
  )
  .unwrap();
  sibling.model.name = "nonlight_child".into();
  sibling.model.lod_lights.clear();
  fs::write(
    stream.join("nonlight_child.ymap"),
    crate::core::format::ymap::binary::write_ymap(&sibling.model, &[], &native_catalog()).unwrap(),
  )
  .unwrap();
  let inventory = inventory(&source, &root.join("cache")).unwrap();
  let (export, mut maps) = export::uncached_snapshot(
    "[group]/resource",
    "nested/child.ymap",
    &inventory["[group]/resource"],
  )
  .unwrap();
  assert_eq!(export.files.len(), 3);
  assert!(export.lights.iter().all(|light| light.lod.len() == 2 && light.editable));
  assert!(export.lights.iter().flat_map(|light| &light.lod).all(|view| view.aabb.min_z <= -10.0));
  let edit = YmapLightEditDocument {
    format: "mlo_lod_light_edits".into(),
    schema_version: 1,
    resource: export.resource.clone(),
    path: export.path.clone(),
    files: export.files.clone(),
    operations: vec![
      LightOperation::Remove {
        light_id: export.lights[0].id.clone(),
      },
      LightOperation::Remove {
        light_id: export.lights[2].id.clone(),
      },
      LightOperation::Remove {
        light_id: export.lights[2].id.clone(),
      },
    ],
  };
  edits::apply(&export, &edit, &mut maps).unwrap();
  assert_eq!(maps["nested/child.ymap"].model.lod_lights.len(), 1);
  assert_eq!(maps["nested/second_child.ymap"].model.lod_lights.len(), 1);
  assert_eq!(maps["nested/parent.ymap"].model.distant_lod_lights.items[0].position, "1 0 0");
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn cached_baseline_inputs_accept_combined_removals_and_keep_unchanged_provider() {
  use crate::core::vanilla::{
    CacheVersion, CachedFile, FileChange, VanillaCacheManifest, write_json,
  };
  use sha2::{Digest, Sha256};
  use std::fs;
  let root = std::env::temp_dir().join(format!("light_baseline_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let source = native_resources(&root, &["a", "b"]);
  let vanilla = root.join("vanilla");
  let cache = root.join("vanilla-cache");
  let source_cache = root.join("source-cache");
  let output = root.join("merged");
  let overwrite = root.join("overwrite");
  fs::create_dir_all(vanilla.join("0000-base/ymap")).unwrap();
  let mut changes = BTreeMap::new();
  for name in ["child.ymap", "parent.ymap"] {
    let bytes = fs::read(source.join("a/stream/nested").join(name)).unwrap();
    let object = format!("0000-base/ymap/{name}");
    fs::write(vanilla.join(&object), &bytes).unwrap();
    changes.insert(
      name.into(),
      FileChange {
        previous_sha256: None,
        file: CachedFile {
          sha256: format!("{:x}", Sha256::digest(&bytes)),
          object,
          source: "fixture.rpf".into(),
        },
      },
    );
  }
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
        changes,
      }],
    },
  )
  .unwrap();
  let run = || {
    crate::core::merge::merge::run_with_overwrite(
      &source,
      &vanilla,
      &cache,
      &source_cache,
      &output,
      false,
      &overwrite,
    )
    .unwrap()
  };
  run();
  assert!(!source.join("a/stream/nested/child.ymap").exists());
  let files = inventory(&source, &source_cache).unwrap();
  let command = ExportLights {
    input: files["a"]["nested/child.ymap"].path.clone(),
    resource: "a".into(),
    source_dir: source.clone(),
    source_cache_dir: source_cache.clone(),
    output_dir: root.join("json"),
    force: false,
  };
  let export: YmapLightDocument =
    serde_json::from_reader(fs::File::open(command.run().unwrap().path).unwrap()).unwrap();
  let (parent_export, _) =
    export::uncached_snapshot("a", "nested/parent.ymap", &files["a"]).unwrap();
  fs::create_dir_all(overwrite.join("a/nested")).unwrap();
  for (snapshot, remove) in [(&export, 0), (&parent_export, 1)] {
    let edit = YmapLightEditDocument {
      format: "mlo_lod_light_edits".into(),
      schema_version: 1,
      resource: "a".into(),
      path: snapshot.path.clone(),
      files: snapshot.files.clone(),
      operations: vec![LightOperation::Remove {
        light_id: snapshot.lights[remove].id.clone(),
      }],
    };
    write_json(&overwrite.join("a").join(format!("{}.diff.json", snapshot.path)), &edit).unwrap();
  }
  run();
  let read = |name: &str| crate::core::vanilla::read_ymap(&output.join("ymap").join(name)).unwrap();
  assert_eq!(read("parent.ymap").distant_lod_lights.items.len(), 3);
  fs::remove_dir_all(source.join("b")).unwrap();
  run();
  assert_eq!(read("parent.ymap").distant_lod_lights.items.len(), 1);
  assert_eq!(read("child.ymap").lod_lights.len(), 1);
  assert_eq!(read("parent.ymap").distant_lod_lights.items[0].position, "2 0 0");
  fs::remove_file(overwrite.join("a/nested/parent.ymap.diff.json")).unwrap();
  run();
  assert_eq!(read("parent.ymap").distant_lod_lights.items.len(), 2);
  let mut remove_all = YmapLightEditDocument {
    format: "mlo_lod_light_edits".into(),
    schema_version: 1,
    resource: "a".into(),
    path: export.path.clone(),
    files: export.files.clone(),
    operations: Vec::new(),
  };
  remove_all.operations = export
    .lights
    .iter()
    .map(|light| LightOperation::Remove {
      light_id: light.id.clone(),
    })
    .collect();
  write_json(&overwrite.join("a/nested/child.ymap.diff.json"), &remove_all).unwrap();
  run();
  assert!(read("child.ymap").lod_lights.is_empty());
  assert!(read("parent.ymap").distant_lod_lights.items.is_empty());
  assert_eq!(read("parent.ymap").distant_lod_lights.num_street_lights, 0);
  fs::remove_dir_all(&overwrite).unwrap();
  run();
  assert!(!output.join("ymap/child.ymap").exists());
  assert_eq!(
    crate::core::vanilla::read_ymap(&files["a"]["nested/child.ymap"].path)
      .unwrap()
      .lod_lights
      .len(),
    3
  );
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn light_snapshots_reject_malformed_native_soa_without_changing_general_reader() {
  use crate::core::format::gamefile::{meta_resource::MetaResource, resource_file::Rsc7Resource};
  use std::fs;
  let root = std::env::temp_dir().join(format!("light_malformed_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let source = native_resources(&root, &["resource"]);
  let path = source.join("resource/stream/nested/parent.ymap");
  let resource = Rsc7Resource::decode(&fs::read(&path).unwrap()).unwrap();
  let mut meta = MetaResource::parse(&resource).unwrap();
  let root_index = (meta.root_block_index - 1) as usize;
  let rgbi_count_offset = 216 + 24 + 8;
  meta.data_blocks[root_index].data[rgbi_count_offset..rgbi_count_offset + 2]
    .copy_from_slice(&2u16.to_le_bytes());
  fs::write(&path, meta.to_rsc7(2).unwrap().encode().unwrap()).unwrap();
  assert_eq!(crate::core::vanilla::read_ymap(&path).unwrap().distant_lod_lights.items.len(), 2);
  let input = resource::ResourceFile {
    path: path.clone(),
    original: path,
    parent_hint: None,
  };
  assert!(read_map(&input).err().unwrap().to_string().contains("position/RGBI lengths differ"));
  fs::remove_dir_all(root).unwrap();
}
