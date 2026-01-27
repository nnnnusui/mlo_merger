use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// Recursively collects all files with the specified suffix from a directory
pub fn collect_files_with_suffix(dir: &Path, suffix: &str) -> Vec<PathBuf> {
  WalkDir::new(dir)
    .into_iter()
    .filter_map(|e| e.ok())
    .map(|e| e.path().to_path_buf())
    .filter(|p| {
      p.is_file()
        && p
          .file_name()
          .and_then(|name| name.to_str())
          .map(|name| name.to_lowercase().ends_with(&suffix.to_lowercase()))
          .unwrap_or(false)
    })
    .collect()
}
