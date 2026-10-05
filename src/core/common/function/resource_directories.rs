use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Discovers a single FiveM resource or manifest-bearing resources inside bracket groups.
///
/// ```no_run
/// let resources = mlo_merger::core::common::function::get_resource_directories(
///   std::path::Path::new("asset/source"),
/// )?;
/// # Ok::<(), std::io::Error>(())
/// ```
pub fn get_resource_directories(input: &Path) -> io::Result<Vec<PathBuf>> {
  fn is_resource(path: &Path) -> bool {
    path.join("fxmanifest.lua").is_file() || path.join("__resource.lua").is_file()
  }
  fn explore(
    path: &Path,
    resources: &mut Vec<PathBuf>,
  ) -> io::Result<()> {
    if is_resource(path) {
      resources.push(path.to_path_buf());
      return Ok(());
    }
    for entry in fs::read_dir(path)? {
      let entry = entry?;
      if !entry.file_type()?.is_dir() {
        continue;
      }
      let child = entry.path();
      if is_resource(&child) {
        resources.push(child);
      } else if entry.file_name().to_string_lossy().starts_with('[')
        && entry.file_name().to_string_lossy().ends_with(']')
      {
        explore(&child, resources)?;
      }
    }
    Ok(())
  }
  let mut resources = Vec::new();
  explore(input, &mut resources)?;
  resources.sort_by_key(|path| (path.to_string_lossy().to_lowercase(), path.clone()));
  Ok(resources)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn resource_discovery_handles_single_multiple_and_nested_groups() {
    let root = std::env::temp_dir().join(format!("resource_discovery_{}", std::process::id()));
    let first = root.join("[group]/[nested]/first");
    let second = root.join("second");
    fs::create_dir_all(first.join("stream")).unwrap();
    fs::create_dir_all(&second).unwrap();
    fs::create_dir_all(root.join("ignored/stream")).unwrap();
    fs::write(first.join("fxmanifest.lua"), []).unwrap();
    fs::write(second.join("__resource.lua"), []).unwrap();
    assert_eq!(get_resource_directories(&first).unwrap(), vec![first.clone()]);
    assert_eq!(get_resource_directories(&root).unwrap(), vec![first, second]);
    assert!(get_resource_directories(&root.join("missing")).is_err());
    fs::remove_dir_all(root).unwrap();
  }
}
