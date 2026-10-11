//! Collects conflicts in active cache files and source resource streams.

use super::{
  Result,
  filesystem::ensure_safe_path,
  types::{ResourceInventory, SourceCacheConflictReport},
};
use crate::core::stream_conflicts::{StreamConflictReport, StreamFileConflict};
use std::{
  collections::{BTreeMap, BTreeSet},
  path::Path,
};

pub(super) fn conflicts(
  source: &Path,
  output: &Path,
  inventories: &BTreeMap<String, ResourceInventory>,
  resources: &[std::path::PathBuf],
) -> Result<SourceCacheConflictReport> {
  let mut names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
  let mut scanned_file_count = 0;
  for inventory in inventories.values() {
    let name = inventory.source.file_name().ok_or("Resource has no name")?;
    let current = output.join("resources").join(name);
    let root = if current.is_dir() { current } else { output.join(name) };
    ensure_safe_path(output, &root)?;
    if !root.is_dir() {
      continue;
    }
    for entry in walkdir::WalkDir::new(&root).follow_links(false) {
      let entry = entry?;
      if !entry.file_type().is_file() {
        continue;
      }
      let path = entry.path();
      let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Cache filename is not UTF-8")?
        .to_ascii_lowercase();
      let relative = path.strip_prefix(output)?.to_string_lossy().replace('\\', "/");
      scanned_file_count += 1;
      names.entry(name).or_default().insert(relative);
    }
  }
  let conflicts = group_conflicts(names);
  Ok(SourceCacheConflictReport {
    report: StreamConflictReport {
      input_dir: output.to_string_lossy().into_owned(),
      scanned_file_count,
      conflict_count: conflicts.values().map(Vec::len).sum(),
      conflicts,
    },
    source_conflicts: Some(
      crate::core::stream_conflicts::scan_resource_stream_conflicts(source, resources)?.conflicts,
    ),
  })
}

fn group_conflicts(
  names: BTreeMap<String, BTreeSet<String>>
) -> BTreeMap<String, Vec<StreamFileConflict>> {
  let mut conflicts: BTreeMap<String, Vec<StreamFileConflict>> = BTreeMap::new();
  for (file_name, paths) in names {
    if paths.len() < 2 {
      continue;
    }
    let extension = Path::new(&file_name)
      .extension()
      .and_then(|value| value.to_str())
      .map(|value| format!(".{value}"))
      .unwrap_or_else(|| "[no extension]".into());
    conflicts.entry(extension).or_default().push(StreamFileConflict {
      file_name,
      paths: paths.into_iter().collect(),
    });
  }
  conflicts
}
