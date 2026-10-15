use std::{
  collections::HashMap,
  path::{Path, PathBuf},
};

use walkdir::WalkDir;

use crate::core::format::stream_file::{FileType, StreamFile};

/// Collects every YMAP and YBN file below `asset/<directory>`, following symlinks.
fn stream_files(directory: &str) -> Vec<PathBuf> {
  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset").join(directory);
  assert!(root.is_dir(), "missing {}", root.display());
  let mut paths = WalkDir::new(&root)
    .follow_links(true)
    .into_iter()
    .map(|entry| entry.unwrap().into_path())
    .filter(|path| {
      path.is_file() && matches!(FileType::from_path(path), Ok(FileType::Ymap | FileType::Ybn))
    })
    .collect::<Vec<_>>();
  paths.sort();
  paths
}

/// Reads all files across threads and fails listing every path that could not be decoded.
fn assert_all_readable(directory: &str) {
  let paths = stream_files(directory);
  assert!(!paths.is_empty(), "no stream files in asset/{directory}");
  let threads = std::thread::available_parallelism().map_or(1, usize::from);
  let chunk = paths.len().div_ceil(threads);
  let failures = std::thread::scope(|scope| {
    paths
      .chunks(chunk)
      .map(|paths| {
        scope.spawn(|| {
          paths
            .iter()
            .filter_map(|path| {
              StreamFile::read(path, &HashMap::new())
                .err()
                .map(|error| format!("{}: {error}", path.display()))
            })
            .collect::<Vec<_>>()
        })
      })
      .collect::<Vec<_>>()
      .into_iter()
      .flat_map(|handle| handle.join().unwrap())
      .collect::<Vec<_>>()
  });
  assert!(
    failures.is_empty(),
    "{} of {} files in asset/{directory} failed:\n{}",
    failures.len(),
    paths.len(),
    failures.join("\n")
  );
}

#[test]
#[ignore = "reads every local vanilla-cache stream file"]
fn reads_all_vanilla_cache_latest_stream_files() {
  assert_all_readable("vanilla-cache/latest");
}

#[test]
#[ignore = "reads every local source stream file"]
fn reads_all_source_stream_files() {
  assert_all_readable("source");
}

#[test]
#[ignore = "reads every local source-cache stream file"]
fn reads_all_source_cache_stream_files() {
  assert_all_readable("source-cache");
}
