use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};

struct Fixture {
  root: PathBuf,
}

impl Fixture {
  /// Two resources and a two-version vanilla archive, built from the `docs/sample` YMAPs.
  ///
  /// `[maps]/alpha` holds `parent.ymap` twice (`a/` and `b/`) and a cached `child.ymap`;
  /// `beta` holds `parent.ymap` once. Vanilla `0000-base` has `parent.ymap`, and `0001-update`
  /// adds `child.ymap`.
  fn new(name: &str) -> Self {
    let root = std::env::temp_dir().join(format!("mlo_read_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let sample = |name: &str| {
      fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/parent_refs").join(name))
        .unwrap()
    };
    let write = |path: &Path, bytes: &[u8]| {
      fs::create_dir_all(path.parent().unwrap()).unwrap();
      fs::write(path, bytes).unwrap();
    };
    let parent = sample("vanilla_parent.ymap");
    let child = sample("child.ymap");
    let alpha = root.join("source/[maps]/alpha");
    let beta = root.join("source/beta");
    write(&alpha.join("stream/a/parent.ymap"), &parent);
    write(&alpha.join("stream/b/parent.ymap"), &sample("resource_a_parent.ymap"));
    write(&root.join("source-cache/resources/alpha/child.ymap"), &child);
    write(&beta.join("stream/parent.ymap"), &sample("resource_b_parent.ymap"));
    let file = |path: &str, cached: Option<&str>| {
      json!({
        "path": path, "file_name": Path::new(path).file_name().unwrap().to_str().unwrap(),
        "sha256": "", "size": 0, "modified_seconds": 0, "modified_nanos": 0,
        "vanilla": null, "ymap_parent_hash": null, "metadata_error": null, "cached_path": cached,
      })
    };
    write(
      &root.join("source-cache/cache_info.json"),
      &serde_json::to_vec(&json!({
        "format_version": 3, "source_dir": root.join("source"), "vanilla_dir": "",
        "vanilla_cache_dir": "", "vanilla_manifest_sha256": "", "vanilla_cache_revision": "",
        "latest_vanilla_version": "0001-update", "generated_at": "", "source_inputs": {},
        "resources": {
          "[maps]/alpha": {"source": alpha, "files_by_format": {".ymap": [
            file("stream/a/parent.ymap", None),
            file("stream/b/parent.ymap", None),
            file("stream/child.ymap", Some("resources/alpha/child.ymap")),
          ], ".ybn": [file("stream/collision.ybn", None)]}},
          "beta": {"source": beta, "files_by_format": {".ymap": [
            file("stream/parent.ymap", None),
          ]}}
        },
        "scanned_stream_file_count": 0, "conflict_count": 0,
        "ymap_load_plan": {"changed_source_parents": [], "additional_source_children": []},
        "outputs": [],
      }))
      .unwrap(),
    );
    let vanilla = root.join("vanilla");
    write(&vanilla.join("0000-base/ymap/parent.ymap"), &parent);
    write(&vanilla.join("0001-update/ymap/child.ymap"), &child);
    write(&vanilla.join("hash_names.json"), br#"{"names":{"2BCE4992":"hei_ch1_lod"}}"#);
    let change = |object: &str, bytes: &[u8]| {
      json!({"previous_sha256": null, "file": {
        "sha256": hex::encode(Sha256::digest(bytes)), "object": object, "source": object,
      }})
    };
    write(
      &vanilla.join("cache_info.json"),
      &serde_json::to_vec(&json!({
        "format_version": 1, "game_dir": "", "versions": [
          {"id": "0000-base", "parent": null, "archives": [], "unchanged": 0,
            "changes": {"parent.ymap": change("0000-base/ymap/parent.ymap", &parent)}},
          {"id": "0001-update", "parent": "0000-base", "archives": [], "unchanged": 0,
            "changes": {"child.ymap": change("0001-update/ymap/child.ymap", &child)}},
        ]
      }))
      .unwrap(),
    );
    Self {
      root,
    }
  }

  fn request(
    &self,
    target: ReadTarget,
    files: &[&str],
  ) -> Read {
    Read {
      target,
      files: files.iter().map(|file| file.to_string()).collect(),
      source_dir: self.root.join("source"),
      source_cache_dir: self.root.join("source-cache"),
      vanilla_dir: self.root.join("vanilla"),
      vanilla_cache_dir: self.root.join("vanilla-cache"),
      gamebuild: None,
      output_dir: self.root.join("read-cache"),
      force: false,
      version: None,
    }
  }

  fn read(
    &self,
    target: ReadTarget,
    files: &[&str],
  ) -> Result<ReadResult> {
    self.request(target, files).run()
  }

  /// Records `version` as the stage the vanilla cache was built for.
  fn build_vanilla_cache_for(
    &self,
    version: &str,
  ) {
    let cache = self.root.join("vanilla-cache");
    fs::create_dir_all(&cache).unwrap();
    fs::write(
      cache.join("cache_info.json"),
      serde_json::to_vec(&json!({
        "format_version": 4, "vanilla_manifest_sha256": "", "latest_version": version,
        "vanilla_input_timestamps": {}, "files": {},
      }))
      .unwrap(),
    )
    .unwrap();
  }

  fn outputs(result: &ReadResult) -> Vec<String> {
    result.files.iter().map(|file| relative(&file.output)).collect()
  }
}

impl Drop for Fixture {
  fn drop(&mut self) {
    let _ = fs::remove_dir_all(&self.root);
  }
}

fn relative(path: &Path) -> String {
  let path = path.to_string_lossy().replace('\\', "/");
  path[path.find("read-cache/").unwrap() + "read-cache/".len()..].to_string()
}

#[test]
fn resource_files_are_written_once_as_minified_models_with_their_source() {
  let fixture = Fixture::new("resource");
  let result = fixture.read(ReadTarget::Resource("alpha".into()), &["child.ymap"]).unwrap();
  assert_eq!(Fixture::outputs(&result), ["resources/alpha/child.ymap.json"]);
  let file = &result.files[0];
  assert!(file.updated);
  assert_eq!(
    file.origin,
    ReadOrigin::Resource {
      resource: "alpha".into(),
      stream_path: "child.ymap".into(),
    }
  );
  assert_eq!(file.input, fixture.root.join("source-cache/resources/alpha/child.ymap"));
  let text = fs::read_to_string(&file.output).unwrap();
  assert!(!text.contains('\n'));
  let document: YmapReadDocument = serde_json::from_str(&text).unwrap();
  // The document names its file relative to the source cache, which holds it.
  assert_eq!(document.source.path, Path::new("resources/alpha/child.ymap"));
  assert_eq!(document.source.sha256, hex::encode(Sha256::digest(fs::read(&file.input).unwrap())));
  assert!(document.hash_names.is_some());
  assert_eq!(document.model.entities[0].guid, 300);
  assert_eq!(document.model.parent, "parent");

  let again = fixture.read(ReadTarget::Resource("[maps]/alpha".into()), &["child.ymap"]).unwrap();
  assert!(!again.files[0].updated);
  let forced = Read {
    force: true,
    ..fixture.request(ReadTarget::Resource("alpha".into()), &["child.ymap"])
  }
  .run()
  .unwrap();
  assert!(forced.files[0].updated);

  // A changed source is read again.
  let source = &file.input;
  fs::write(source, fs::read(fixture.root.join("source/beta/stream/parent.ymap")).unwrap())
    .unwrap();
  let changed = fixture.read(ReadTarget::Resource("alpha".into()), &["child.ymap"]).unwrap();
  assert!(changed.files[0].updated);
  let document: YmapReadDocument =
    serde_json::from_str(&fs::read_to_string(&changed.files[0].output).unwrap()).unwrap();
  assert_eq!(document.model.entities[0].guid, 200);
}

#[test]
fn a_resource_reads_every_ymap_and_rejects_ambiguous_names() {
  let fixture = Fixture::new("ambiguous");
  let all = fixture.read(ReadTarget::Resource("alpha".into()), &[]).unwrap();
  assert_eq!(
    Fixture::outputs(&all),
    [
      "resources/alpha/a/parent.ymap.json",
      "resources/alpha/b/parent.ymap.json",
      "resources/alpha/child.ymap.json"
    ]
  );
  let error = fixture.read(ReadTarget::Resource("alpha".into()), &["PARENT.ymap"]).unwrap_err();
  assert!(error.to_string().contains("a/parent.ymap, b/parent.ymap"), "{error}");
  let by_path = fixture.read(ReadTarget::Resource("alpha".into()), &["b/parent.ymap"]).unwrap();
  assert_eq!(Fixture::outputs(&by_path), ["resources/alpha/b/parent.ymap.json"]);
  assert!(fixture.read(ReadTarget::Resource("gamma".into()), &[]).is_err());
  assert!(fixture.read(ReadTarget::Resource("alpha".into()), &["missing.ymap"]).is_err());
  assert!(fixture.read(ReadTarget::Resource("alpha".into()), &["collision.ybn"]).is_err());
}

#[test]
fn vanilla_files_are_read_from_the_last_stage_without_a_vanilla_cache() {
  let fixture = Fixture::new("vanilla");
  // Each file is written after the version folder holding it, not the version read.
  let latest = fixture.read(ReadTarget::Vanilla, &[]).unwrap();
  assert_eq!(
    Fixture::outputs(&latest),
    ["vanilla/0001-update/ymap/child.ymap.json", "vanilla/0000-base/ymap/parent.ymap.json"]
  );
  assert_eq!(
    latest.files[1].origin,
    ReadOrigin::Vanilla {
      version: "0000-base".into(),
      file_name: "parent.ymap".into(),
    }
  );
  assert!(latest.files.iter().all(|file| file.updated));
}

#[test]
fn vanilla_files_follow_the_vanilla_cache_or_the_requested_gamebuild() {
  let fixture = Fixture::new("vanilla_stage");
  fixture.read(ReadTarget::Vanilla, &[]).unwrap();

  // A cache built for the base stage reads its files, which do not include child.ymap; the
  // base stage holds the same parent.ymap, so its document is already current.
  fixture.build_vanilla_cache_for("0000-base");
  let base = fixture.read(ReadTarget::Vanilla, &[]).unwrap();
  assert_eq!(Fixture::outputs(&base), ["vanilla/0000-base/ymap/parent.ymap.json"]);
  assert!(!base.files[0].updated);
  assert!(fixture.read(ReadTarget::Vanilla, &["child.ymap"]).is_err());

  // A game build overrides the cache: build 1 ends at the title update, holding child.ymap.
  let builds = crate::core::config::gamebuild::GameBuilds::parse("[gamebuild]\n1 = []").unwrap();
  let request = Read {
    gamebuild: Some(builds.resolve("1").unwrap()),
    ..fixture.request(ReadTarget::Vanilla, &["child.ymap"])
  };
  assert_eq!(
    Fixture::outputs(&request.run().unwrap()),
    ["vanilla/0001-update/ymap/child.ymap.json"]
  );

  // A cache built for a stage the archive lacks falls back to the last stage.
  fixture.build_vanilla_cache_for("9999-missing");
  assert_eq!(fixture.read(ReadTarget::Vanilla, &[]).unwrap().files.len(), 2);
}

#[test]
fn a_file_alone_is_read_from_every_resource_and_the_latest_vanilla() {
  let fixture = Fixture::new("everywhere");
  let result = fixture.read(ReadTarget::Everywhere, &["child.ymap"]).unwrap();
  assert_eq!(
    Fixture::outputs(&result),
    ["resources/alpha/child.ymap.json", "vanilla/0001-update/ymap/child.ymap.json"]
  );
  // alpha holds two parent.ymap files, so nothing is written.
  assert!(fixture.read(ReadTarget::Everywhere, &["parent.ymap"]).is_err());
  assert!(!fixture.root.join("read-cache/resources/beta").exists());
  assert!(fixture.read(ReadTarget::Everywhere, &[]).is_err());
}

#[test]
fn all_reads_every_resource_and_the_latest_vanilla_once() {
  let fixture = Fixture::new("all");
  let result = fixture.read(ReadTarget::All, &[]).unwrap();
  let mut outputs = Fixture::outputs(&result);
  outputs.sort();
  assert_eq!(
    outputs,
    [
      "resources/alpha/a/parent.ymap.json",
      "resources/alpha/b/parent.ymap.json",
      "resources/alpha/child.ymap.json",
      "resources/beta/parent.ymap.json",
      "vanilla/0000-base/ymap/parent.ymap.json",
      "vanilla/0001-update/ymap/child.ymap.json",
    ]
  );
  assert!(result.files.iter().all(|file| file.updated));
  let again = fixture.read(ReadTarget::All, &[]).unwrap();
  assert_eq!(Fixture::outputs(&again), Fixture::outputs(&result));
  assert!(again.files.iter().all(|file| !file.updated));
  assert!(fixture.read(ReadTarget::All, &["child.ymap"]).is_err());
}

#[test]
fn several_files_are_read_together_and_each_must_match() {
  let fixture = Fixture::new("several");
  let result = fixture
    .read(ReadTarget::Resource("alpha".into()), &["child.ymap", "b/parent.ymap", "CHILD.ymap"])
    .unwrap();
  assert_eq!(
    Fixture::outputs(&result),
    ["resources/alpha/child.ymap.json", "resources/alpha/b/parent.ymap.json"]
  );
  let everywhere = fixture.read(ReadTarget::Everywhere, &["child.ymap", "b/parent.ymap"]).unwrap();
  assert_eq!(
    Fixture::outputs(&everywhere),
    [
      "resources/alpha/child.ymap.json",
      "resources/alpha/b/parent.ymap.json",
      "vanilla/0001-update/ymap/child.ymap.json"
    ]
  );
  let vanilla = fixture.read(ReadTarget::Vanilla, &["parent.ymap", "child.ymap"]).unwrap();
  assert_eq!(Fixture::outputs(&vanilla).len(), 2);

  // A name matching nothing fails the whole read before anything is written.
  fs::remove_dir_all(fixture.root.join("read-cache")).unwrap();
  let error = fixture
    .read(ReadTarget::Resource("alpha".into()), &["child.ymap", "missing.ymap"])
    .unwrap_err();
  assert!(error.to_string().contains("missing.ymap"), "{error}");
  assert!(!fixture.root.join("read-cache/resources").exists());
}
