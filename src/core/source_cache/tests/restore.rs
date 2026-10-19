use super::*;

#[test]
fn restoring_some_resources_keeps_the_others_cached_until_the_next_generation() {
  use crate::core::source_cache::{
    load_merge_inputs, load_moved_source_paths, restore_source_cache,
  };
  use crate::core::vanilla::{CachedFile, FileChange};
  use sha2::{Digest, Sha256};

  let root = std::env::temp_dir().join(format!("source_restore_some_{}", std::process::id()));
  let _ = fs::remove_dir_all(&root);
  let source = root.join("source");
  let vanilla = root.join("vanilla");
  let output = root.join("source-cache");
  for name in ["resource_a", "resource_b"] {
    let resource = source.join(name);
    fs::create_dir_all(resource.join("stream")).unwrap();
    fs::write(resource.join("fxmanifest.lua"), b"fx_version 'cerulean'").unwrap();
    fs::write(resource.join("stream/collision.ybn"), name).unwrap();
  }
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
              sha256: hex::encode(Sha256::digest(b"original")),
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
    resource: None,
    exclusions: None,
    force: false,
  };
  assert!(build.run_after_vanilla_cache().unwrap());
  let moved = || load_moved_source_paths(&output).unwrap();
  let both = BTreeSet::from([
    "resource_a/stream/collision.ybn".to_string(),
    "resource_b/stream/collision.ybn".to_string(),
  ]);
  assert_eq!(moved(), both);

  assert_eq!(restore_source_cache(&output, &source, &["resource_a".into()]).unwrap(), 1);
  assert_eq!(fs::read(source.join("resource_a/stream/collision.ybn")).unwrap(), b"resource_a");
  assert!(output.join("resources/resource_b/collision.ybn").is_file());
  assert_eq!(moved(), BTreeSet::from(["resource_b/stream/collision.ybn".to_string()]));
  // The inventory still locates every file, but merges wait for the next generation.
  let located = crate::core::source_cache::inventoried_files(&output).unwrap();
  assert_eq!(
    located.iter().map(|file| file.path.clone()).collect::<BTreeSet<_>>(),
    BTreeSet::from([
      output.join("resources/resource_b/collision.ybn"),
      source.join("resource_a/stream/collision.ybn"),
    ])
  );
  assert!(load_merge_inputs(&output).is_err());

  // The next generation caches the restored resource again.
  assert!(build.run_after_vanilla_cache().unwrap());
  assert!(!source.join("resource_a/stream/collision.ybn").exists());
  assert_eq!(fs::read(output.join("resources/resource_a/collision.ybn")).unwrap(), b"resource_a");
  assert_eq!(moved(), both);
  assert_eq!(load_merge_inputs(&output).unwrap().ybn.len(), 2);
  assert!(!build.run_after_vanilla_cache().unwrap());
  fs::remove_dir_all(root).unwrap();
}
