use super::*;

pub(super) fn latest_ymap_paths(files: &[PathBuf]) -> HashMap<u32, PathBuf> {
  let mut paths = HashMap::new();
  for path in files {
    if let Some(name) = path.file_stem().and_then(|name| name.to_str()) {
      paths.entry(reference_hash(name)).or_insert_with(|| path.clone());
    }
  }
  paths
}

/// Information about a modded ymap file
#[derive(Debug, Clone)]
pub(super) struct ModYmapReference {
  pub(super) mod_name: String,
  pub(super) ymap_name: String,
  pub(super) mod_ymap_path: PathBuf,
  /// The source cache's diff document of this YMAP, when it comes from a source cache.
  pub(super) diff_path: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn latest_ymap_log_paths_use_actual_native_files() {
    let path = PathBuf::from("vanilla-cache/latest/ymap/vw_lodlights_small037.ymap");
    let paths = latest_ymap_paths(std::slice::from_ref(&path));
    let internal_name = "vw_lodlights_small037.ymap.xml";
    let hash = reference_hash(internal_name.trim_end_matches(".ymap.xml"));
    assert_eq!(paths[&hash], path);
    assert_eq!(paths[&hash].extension().unwrap(), "ymap");
    let duplicate = PathBuf::from("another-cache/vw_lodlights_small037.ymap");
    assert_eq!(latest_ymap_paths(&[path.clone(), duplicate])[&hash], path);
  }
}
