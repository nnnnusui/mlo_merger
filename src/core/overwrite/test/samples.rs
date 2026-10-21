//! The samples under `docs/sample/source` and `docs/sample/overwrite`, built from
//! `docs/sample/sample.ymap` as vanilla `hei_ch1_11.ymap`.
//!
//! The test checks that the samples are what this module builds and that every overwrite applies
//! to the source cache's diffs without a stale fingerprint or an unmatched selector. Run it with
//! `MLO_WRITE_SAMPLES=1` to write the samples instead.

use super::*;
use crate::core::format::gamefile::test_support::sample_ymap;
use crate::core::format::ymap::binary::write_ymap;
use crate::core::format::ymap::compare::{YmapComparisonDiff, YmapComparisonFile};
use crate::core::format::ymap::model::ymap::Ymap;
use serde_json::json;
use std::path::PathBuf;

const VANILLA_NAME: &str = "hei_ch1_11.ymap";

fn docs() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample")
}

/// A sample source resource: its source-relative ID and edited YMAP.
struct SampleSource {
  resource: &'static str,
  stream_path: &'static str,
  model: Ymap,
  /// What its overwrite leaves out of its diff.
  skip: serde_json::Value,
}

fn sample_sources() -> Vec<SampleSource> {
  let vanilla = sample_ymap("sample.ymap");
  let first = vanilla.entities[0].guid;

  // Resource A sets a content flag, moves and re-flags the first entity, and removes the box
  // occluder and triangles 10 and 11 of the occlude model. Its overwrite keeps the entity's new
  // flags and the removal of triangle 11, and leaves out the rest.
  let mut a = vanilla.clone();
  a.content_flags |= 1 << 6;
  a.entities[0].position.z += 2.0;
  a.entities[0].flags ^= 1 << 15;
  a.box_occluders.clear();
  a.occlude_models[0].triangles.remove(11);
  a.occlude_models[0].triangles.remove(10);
  let a_skip = json!({
    "content_flags": true,
    "entities": [{"Modified": {"guid": first, "changes": {"position": true}}}],
    "box_occluders": [{"Removed": {"index": 0}}],
    "occlude_models": [
      {"Modified": {"before_index": 0, "changes": {"triangles": [{"Removed": {"index": 10}}]}}}
    ],
  });

  // Resource B adds a copy of the first entity and removes triangle 20; its overwrite leaves the
  // added entity out.
  let mut b = vanilla.clone();
  let mut added = b.entities[0].clone();
  added.guid = 0x5A3D_0001;
  added.position.x += 10.0;
  b.entities.push(added);
  b.occlude_models[0].triangles.remove(20);
  let b_skip = json!({"entities": [{"Added": {"entity": {"guid": 0x5A3D_0001}}}]});

  vec![
    SampleSource {
      resource: "[maps]/sample_resource_a",
      stream_path: VANILLA_NAME,
      model: a,
      skip: a_skip,
    },
    SampleSource {
      resource: "sample_resource_b",
      stream_path: "maps/hei_ch1_11.ymap",
      model: b,
      skip: b_skip,
    },
  ]
}

fn resource_name(resource: &str) -> &str {
  resource.rsplit('/').next().unwrap()
}

/// Builds the source cache of the samples in a temporary directory and returns its diff
/// documents, with paths made repository-relative and modification times cleared.
fn generate_diffs(
  sources: &BTreeMap<String, Vec<u8>>,
  root: &Path,
) -> BTreeMap<String, YmapComparisonDiff> {
  let source_dir = root.join("docs/sample/source");
  for (path, bytes) in sources {
    let path = source_dir.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
  }
  let vanilla = root.join("docs/sample");
  let vanilla_bytes = fs::read(docs().join("sample.ymap")).unwrap();
  let object = "sample.ymap";
  fs::write(vanilla.join(object), &vanilla_bytes).unwrap();
  fs::copy(docs().join("hash_names.json"), vanilla.join("hash_names.json")).unwrap();
  crate::core::vanilla::write_json(
    &vanilla.join("cache_info.json"),
    &json!({"format_version": 1, "game_dir": "", "versions": [{
      "id": "0000-base", "parent": null, "archives": [], "unchanged": 0,
      "changes": {VANILLA_NAME: {"previous_sha256": null, "file": {
        "sha256": hex::encode(Sha256::digest(&vanilla_bytes)), "object": object,
        "source": format!("base.rpf/{VANILLA_NAME}"),
      }}},
    }]}),
  )
  .unwrap();
  crate::core::source_cache::BuildSourceCache {
    source_dir: source_dir.clone(),
    output_dir: root.join("source-cache"),
    vanilla_dir: vanilla.clone(),
    vanilla_cache_dir: root.join("vanilla-cache"),
    resource: None,
    exclusions: None,
    force: false,
  }
  .run_after_vanilla_cache()
  .unwrap();
  let mut diffs = BTreeMap::new();
  for source in sample_sources() {
    let name = format!("{}/{}.diff.json", resource_name(source.resource), source.stream_path);
    let mut document: YmapComparisonDiff =
      serde_json::from_reader(fs::File::open(root.join("source-cache/diff").join(&name)).unwrap())
        .unwrap();
    let clean = |file: &mut YmapComparisonFile, path: String| {
      file.path = path;
      file.modified_seconds = 0;
      file.modified_nanos = 0;
    };
    // The source cache moved the source into itself; the sample names where it came from.
    clean(
      &mut document.source,
      format!("docs/sample/source/{}/stream/{}", source.resource, source.stream_path),
    );
    clean(&mut document.vanilla, "docs/sample/sample.ymap".into());
    diffs.insert(name, document);
  }
  diffs
}

#[test]
fn sample_sources_and_overwrites_are_current_and_apply_cleanly() {
  let write = std::env::var_os("MLO_WRITE_SAMPLES").is_some();
  let sources = sample_sources()
    .iter()
    .map(|source| {
      (
        format!("{}/stream/{}", source.resource, source.stream_path),
        write_ymap(&source.model, &source.model.entities).unwrap(),
      )
    })
    .chain(sample_sources().iter().map(|source| {
      (
        format!("{}/fxmanifest.lua", source.resource),
        b"fx_version 'cerulean'\ngame 'gta5'\n".to_vec(),
      )
    }))
    .collect::<BTreeMap<_, _>>();

  let root = std::env::temp_dir().join(format!("mlo_overwrite_samples_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  fs::create_dir_all(&root).unwrap();
  let root = root.canonicalize().unwrap();
  // The samples are taken with the default tolerances, whatever the local config says.
  let diffs = crate::core::config::matching::with_tolerances(Default::default(), || {
    generate_diffs(&sources, &root)
  });

  let mut overwrites = BTreeMap::<&str, ResourceOverwrite>::new();
  for source in sample_sources() {
    let document =
      &diffs[&format!("{}/{}.diff.json", resource_name(source.resource), source.stream_path)];
    let fingerprint = |file: &YmapComparisonFile| OverwriteFingerprint {
      sha256: file.sha256.clone(),
      size: file.size,
    };
    overwrites
      .entry(resource_name(source.resource))
      .or_insert_with(|| ResourceOverwrite {
        schema_version: ResourceOverwrite::SCHEMA_VERSION,
        files: BTreeMap::new(),
      })
      .files
      .insert(
        source.stream_path.to_string(),
        StreamFileOverwrite {
          source: fingerprint(&document.source),
          vanilla: fingerprint(&document.vanilla),
          skip: source.skip.clone(),
        },
      );
  }

  let expected = sources
    .iter()
    .map(|(path, bytes)| (format!("source/{path}"), bytes.clone()))
    .chain(
      diffs.iter().map(|(name, document)| (format!("overwrite/diff/{name}"), pretty(document))),
    )
    .chain(overwrites.iter().map(|(resource, overwrite)| {
      (format!("overwrite/resources/{resource}.json"), pretty(overwrite))
    }))
    .collect::<BTreeMap<_, _>>();
  for (path, bytes) in &expected {
    let path = docs().join(path);
    if write {
      fs::create_dir_all(path.parent().unwrap()).unwrap();
      fs::write(&path, bytes).unwrap();
    } else {
      assert!(
        fs::read(&path).is_ok_and(|actual| actual == *bytes),
        "{} is stale; rerun with MLO_WRITE_SAMPLES=1",
        path.display()
      );
    }
  }

  // Every overwrite applies to its diff, and every selector finds its difference.
  let loaded = Overwrites::load(&docs().join("overwrite")).unwrap();
  for source in sample_sources() {
    let name = format!("{}/{}.diff.json", resource_name(source.resource), source.stream_path);
    let document = &diffs[&name];
    let overwrite = loaded.stream_file(resource_name(source.resource), source.stream_path).unwrap();
    let fingerprint = |file: &YmapComparisonFile| OverwriteFingerprint {
      sha256: file.sha256.clone(),
      size: file.size,
    };
    assert_eq!(overwrite.source, fingerprint(&document.source), "{name}");
    assert_eq!(overwrite.vanilla, fingerprint(&document.vanilla), "{name}");
    let mut diff = document.diffs.clone();
    let unmatched = skip_diffs(&mut diff, &overwrite.skip).unwrap();
    assert!(unmatched.is_empty(), "{name}: {unmatched:?}");
    assert_ne!(
      serde_json::to_value(&diff).unwrap(),
      serde_json::to_value(&document.diffs).unwrap(),
      "{name}"
    );
  }
  fs::remove_dir_all(root).unwrap();
}

/// Pretty JSON with a trailing newline, as the samples are written for reading.
fn pretty<T: Serialize>(value: &T) -> Vec<u8> {
  let mut bytes = serde_json::to_vec_pretty(value).unwrap();
  bytes.push(b'\n');
  bytes
}
