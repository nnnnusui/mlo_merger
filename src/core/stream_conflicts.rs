use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::core::extract::get_resource_directories;

/// JSON report containing only basenames that occur more than once.
#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StreamConflictReport {
  /// Input directory used for the scan.
  pub input_dir: String,
  /// Number of unique files scanned under stream directories.
  pub scanned_file_count: usize,
  /// Number of distinct colliding basenames.
  pub conflict_count: usize,
  /// Filename conflicts grouped by lowercase extension (including the leading dot).
  pub conflicts: BTreeMap<String, Vec<StreamFileConflict>>,
}

/// One normalized filename and all relative paths where it occurs.
#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StreamFileConflict {
  /// Basename normalized to lowercase for case-insensitive matching.
  pub file_name: String,
  /// Relative paths to each matching file.
  pub paths: Vec<String>,
}

/// Recursively scans stream directories and groups files by case-insensitive basename.
pub fn scan_stream_conflicts(input_dir: &Path) -> io::Result<StreamConflictReport> {
  if !input_dir.is_dir() {
    return Err(io::Error::new(io::ErrorKind::NotFound, "input directory does not exist"));
  }

  let stream_dirs = find_stream_directories(input_dir)?;
  let mut files_by_name: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
  let mut scanned_paths = BTreeSet::new();

  for stream_dir in stream_dirs {
    for entry in WalkDir::new(&stream_dir).follow_links(false) {
      let entry = entry.map_err(|error| io::Error::other(error.to_string()))?;
      if !entry.file_type().is_file() {
        continue;
      }
      let path = entry.path();
      if !scanned_paths.insert(path.to_path_buf()) {
        continue;
      }
      let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        continue;
      };
      let relative_path =
        path.strip_prefix(input_dir).unwrap_or(path).to_string_lossy().replace('\\', "/");
      files_by_name.entry(file_name.to_lowercase()).or_default().insert(relative_path);
    }
  }

  let mut conflicts: BTreeMap<String, Vec<StreamFileConflict>> = BTreeMap::new();
  for (file_name, paths) in files_by_name {
    if paths.len() <= 1 {
      continue;
    }
    let extension = Path::new(&file_name)
      .extension()
      .and_then(|extension| extension.to_str())
      .map(|extension| format!(".{extension}"))
      .unwrap_or_else(|| "[no extension]".to_string());
    conflicts.entry(extension).or_default().push(StreamFileConflict {
      file_name,
      paths: paths.into_iter().collect(),
    });
  }
  let conflict_count = conflicts.values().map(Vec::len).sum();

  Ok(StreamConflictReport {
    input_dir: input_dir.to_string_lossy().into_owned(),
    scanned_file_count: scanned_paths.len(),
    conflict_count,
    conflicts,
  })
}

fn find_stream_directories(input_dir: &Path) -> io::Result<Vec<PathBuf>> {
  if input_dir.file_name().is_some_and(|name| {
    ["stream", "streams"].iter().any(|stream| name.to_string_lossy().eq_ignore_ascii_case(stream))
  }) {
    return Ok(vec![input_dir.to_path_buf()]);
  }

  let resources =
    if input_dir.join("fxmanifest.lua").is_file() || input_dir.join("__resource.lua").is_file() {
      vec![input_dir.to_path_buf()]
    } else {
      get_resource_directories(&input_dir.to_path_buf())
        .map_err(|error| io::Error::other(error.to_string()))?
    };

  Ok(
    resources
      .into_iter()
      .flat_map(|resource| ["stream", "streams"].into_iter().map(move |name| resource.join(name)))
      .filter(|stream| stream.is_dir())
      .collect(),
  )
}

#[cfg(test)]
mod tests {
  use super::scan_stream_conflicts;

  #[test]
  fn finds_case_insensitive_filename_collisions_across_streams() {
    let temp_dir = std::env::temp_dir().join(format!("stream_conflicts_{}", std::process::id()));
    let first_stream = temp_dir.join("resource_a/stream/ymap");
    let plural_stream = temp_dir.join("resource_a/streams/ymap");
    let second_stream = temp_dir.join("resource_b/stream/other");
    let third_stream = temp_dir.join("resource_b/stream/ytyp");
    let unmanifested_stream = temp_dir.join("not_a_resource/stream");
    std::fs::create_dir_all(&first_stream).unwrap();
    std::fs::create_dir_all(&plural_stream).unwrap();
    std::fs::create_dir_all(&second_stream).unwrap();
    std::fs::create_dir_all(&third_stream).unwrap();
    std::fs::create_dir_all(&unmanifested_stream).unwrap();
    std::fs::write(temp_dir.join("resource_a/fxmanifest.lua"), []).unwrap();
    std::fs::write(temp_dir.join("resource_b/__resource.lua"), []).unwrap();
    std::fs::write(first_stream.join("mission.ymap"), []).unwrap();
    std::fs::write(plural_stream.join("mission.ymap"), []).unwrap();
    std::fs::write(second_stream.join("MISSION.YMAP"), []).unwrap();
    std::fs::write(first_stream.join("unique.ytyp"), []).unwrap();
    std::fs::write(first_stream.join("shared.ytyp"), []).unwrap();
    std::fs::write(third_stream.join("SHARED.YTYP"), []).unwrap();
    std::fs::write(unmanifested_stream.join("mission.ymap"), []).unwrap();

    let report = scan_stream_conflicts(&temp_dir).unwrap();
    assert_eq!(report.scanned_file_count, 6);
    assert_eq!(report.conflict_count, 2);
    assert_eq!(report.conflicts[".ymap"][0].file_name, "mission.ymap");
    assert_eq!(report.conflicts[".ymap"][0].paths.len(), 3);
    assert_eq!(report.conflicts[".ytyp"][0].file_name, "shared.ytyp");
    assert_eq!(report.conflicts[".ytyp"][0].paths.len(), 2);

    std::fs::remove_dir_all(temp_dir).unwrap();
  }
}
