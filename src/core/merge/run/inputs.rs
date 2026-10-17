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

pub(super) fn collect_modded_ymaps_map(
  mod_dir: &Path
) -> Result<BTreeMap<String, Vec<ModYmapReference>>, Box<dyn std::error::Error>> {
  let mod_files = collect_files_with_suffix(mod_dir, ".ymap.xml");
  let mut map: BTreeMap<String, Vec<ModYmapReference>> = BTreeMap::new();

  for mod_file in mod_files {
    // Extract file name
    let file_name = mod_file.file_name().and_then(|n| n.to_str()).ok_or("Invalid file name")?;

    // Extract mod name and ymap name from "modname___ymapname.ymap.xml" format
    if let Some((mod_name, ymap_xml_name)) = file_name.split_once("___") {
      let ymap_name = ymap_xml_name.trim_end_matches(".xml");
      let info = ModYmapReference {
        mod_name: mod_name.to_string(),
        ymap_name: ymap_name.to_string(),
        mod_ymap_path: mod_file.clone(),
        diff_path: None,
      };

      map.entry(ymap_xml_name.to_string()).or_default().push(info);
    } else {
      tracing::warn!(
        "Warning: Invalid file name format (expected 'mod___ymap.ymap.xml'): {}",
        file_name
      );
    }
  }

  Ok(map)
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
