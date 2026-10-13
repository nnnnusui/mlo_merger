use super::*;

#[test]
fn validate_paths_only_rejects_overwrite_overlaps() {
  let root = std::env::temp_dir().join(format!("merge_paths_{}", std::process::id()));
  let merge = |overwrite_dir| Merge {
    source_dir: root.join("source"),
    vanilla_dir: root.join("vanilla"),
    vanilla_cache_dir: root.join("vanilla-cache"),
    source_cache_dir: root.join("source-cache"),
    output_dir: root.join("merged"),
    overwrite_dir,
    exclusions: None,
    force: false,
  };

  assert!(merge(root.join("merged/overwrite")).validate_paths().is_err());
  assert!(merge(root.clone()).validate_paths().is_err());
  assert!(merge(root.join("overwrite")).validate_paths().is_ok());
}
