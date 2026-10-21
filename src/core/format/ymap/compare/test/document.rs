//! End-to-end derivation through [`YmapComparisonContext::get_diff`] over minimal caches.

use std::{fs, path::Path};

use crate::common::fingerprint::Fingerprint;
use crate::core::config::matching::MatchTolerances;
use crate::core::format::ymap::compare::{YmapComparisonDiff, YmapComparisonFile};

use serde_json::json;

use crate::core::format::{
  gamefile::{
    meta::{MetaResource, MetaSchemaCatalog},
    rsc7::Rsc7Resource,
  },
  ymap::{
    binary::{write_ymap, ymap_to_model},
    compare::{StreamFileRef, YmapComparisonContext},
  },
};

#[test]
#[ignore = "requires a local vanilla-cache YMAP"]
fn get_diff_fingerprints_both_files_and_derives_their_differences() {
  let name = "ap1_01_a.ymap";
  let vanilla_bytes = fs::read(
    Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap").join(name),
  )
  .unwrap();
  let mut catalog = MetaSchemaCatalog::default();
  catalog
    .add_resource(&MetaResource::parse(&Rsc7Resource::decode(&vanilla_bytes).unwrap()).unwrap());
  let mut modded = ymap_to_model(&vanilla_bytes, &catalog.hash_names).unwrap();
  modded.flags ^= 2;
  // The last entity, so no other entity's parentIndex shifts; parents compare by the entity
  // they point at, and shifted indices would point at other entities.
  let removed_index = modded.entities.len() - 1;
  assert!(modded.entities.iter().all(|entity| entity.parent_index != removed_index as i32));
  let removed_guid = modded.entities.remove(removed_index).guid;
  let modded_bytes = write_ymap(&modded, &modded.entities).unwrap();

  let root = std::env::temp_dir().join(format!("ymap_compare_document_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let (vanilla_cache, source_cache) = (root.join("vanilla-cache"), root.join("source-cache"));
  fs::create_dir_all(vanilla_cache.join("latest/ymap")).unwrap();
  fs::write(vanilla_cache.join("latest/ymap").join(name), &vanilla_bytes).unwrap();
  fs::write(vanilla_cache.join("cache_info.json"), json!({ "latest_version": "v1" }).to_string())
    .unwrap();
  let cached = format!("resources/mod/stream/{name}");
  fs::create_dir_all(source_cache.join("resources/mod/stream")).unwrap();
  fs::write(source_cache.join(&cached), &modded_bytes).unwrap();
  let metadata = json!({
    "format_version": 3,
    "source_dir": root.join("source"),
    "vanilla_dir": root.join("vanilla"),
    "vanilla_cache_dir": vanilla_cache,
    "vanilla_manifest_sha256": "",
    "vanilla_cache_revision": "",
    "latest_vanilla_version": "v1",
    "generated_at": "",
    "source_inputs": {},
    "resources": {
      "[group]/mod": {
        "source": root.join("source/[group]/mod"),
        "files_by_format": {
          ".ymap": [{
            "path": format!("stream/{name}"),
            "file_name": name,
            "sha256": "",
            "size": 0,
            "modified_seconds": 0,
            "modified_nanos": 0,
            "vanilla": null,
            "ymap_parent_hash": null,
            "metadata_error": null,
            "cached_path": cached,
          }],
        },
      },
    },
    "scanned_stream_file_count": 1,
    "conflict_count": 0,
    "ymap_load_plan": { "changed_source_parents": [], "additional_source_children": [] },
    "outputs": [],
  });
  fs::write(source_cache.join("cache_info.json"), metadata.to_string()).unwrap();

  let mut context =
    YmapComparisonContext::new(&source_cache, &vanilla_cache, catalog.hash_names.clone()).unwrap();
  let file = StreamFileRef {
    resource_name: Some("mod".into()),
    vanilla_version: "v1".into(),
    file_name: name.into(),
  };
  let document = serde_json::to_value(context.get_diff(&file).unwrap()).unwrap();
  let sha256 = |bytes: &[u8]| {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
  };
  assert_eq!(document["source"]["sha256"], json!(sha256(&modded_bytes)));
  assert_eq!(document["source"]["size"], json!(modded_bytes.len()));
  assert_eq!(document["vanilla"]["sha256"], json!(sha256(&vanilla_bytes)));
  assert!(document["source"]["path"].as_str().unwrap().ends_with(&cached));
  assert_eq!(document["diffs"]["flags"], json!(modded.flags));
  assert_eq!(
    document["diffs"]["entities"],
    json!([{ "Removed": { "index": removed_index, "guid": removed_guid } }])
  );
  let unexpected = document["diffs"]
    .as_object()
    .unwrap()
    .keys()
    .filter(|key| !["flags", "entities"].contains(&key.as_str()))
    .cloned()
    .collect::<Vec<_>>();
  assert!(unexpected.is_empty(), "unexpected diffs: {unexpected:?}");
  fs::remove_dir_all(root).unwrap();
}

#[test]
fn only_documents_of_the_current_files_and_tolerances_are_read() {
  let root = std::env::temp_dir().join(format!("ymap_diff_document_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  fs::create_dir_all(&root).unwrap();
  let (source, vanilla, path) =
    (root.join("source.ymap"), root.join("vanilla.ymap"), root.join("doc.json"));
  fs::write(&source, b"source bytes").unwrap();
  fs::write(&vanilla, b"vanilla bytes").unwrap();
  let file = |path: &Path| {
    let path = path.canonicalize().unwrap();
    let fingerprint = Fingerprint::read(&path, None, false).unwrap();
    YmapComparisonFile {
      path: path.display().to_string(),
      sha256: fingerprint.sha256,
      size: fingerprint.size,
      modified_seconds: fingerprint.modified_seconds,
      modified_nanos: fingerprint.modified_nanos,
    }
  };
  let document = YmapComparisonDiff {
    schema_version: YmapComparisonDiff::SCHEMA_VERSION,
    source: file(&source),
    vanilla: file(&vanilla),
    tolerances: MatchTolerances::default(),
    partner: None,
    diffs: Default::default(),
  };
  fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();
  let read = |tolerances: &MatchTolerances| {
    YmapComparisonDiff::read_current(&path, &source, &vanilla, tolerances).unwrap().is_some()
  };
  assert!(read(&MatchTolerances::default()));
  assert!(!read(&MatchTolerances {
    ymap: 0.5,
    ..Default::default()
  }));
  fs::write(&source, b"edited source bytes").unwrap();
  assert!(!read(&MatchTolerances::default()));
  assert!(
    YmapComparisonDiff::read_current(
      &root.join("missing.json"),
      &source,
      &vanilla,
      &Default::default()
    )
    .unwrap()
    .is_none()
  );
  fs::remove_dir_all(root).unwrap();
}
