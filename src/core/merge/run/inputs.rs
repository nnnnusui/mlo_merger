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

pub(super) fn best_cached_ymap_diff(
  history: &VanillaHistory,
  sources: &mut SourceMaps,
  references: &mut ParentReferences,
  hash: u32,
  name: &str,
  modded: &Ymap,
) -> Result<YmapDiff, Box<dyn std::error::Error>> {
  let mut candidates = Vec::new();
  for (version, file) in history.candidate_files(name)? {
    let original = OriginalMap::load_raw(&history.raw_path(&file)?)?;
    let (candidate, entities) = sources.normalize(&original, None, hash, references)?;
    if candidate.entity_map.len() != entities.len() {
      continue;
    }
    candidates.push((version, candidate));
  }
  let (score, version, diff) = best_ymap_diff(candidates, modded)?;
  log::info!("Best YMAP baseline {name}: {version} ({score} changes)");
  Ok(diff)
}

pub(super) fn best_ymap_diff(
  candidates: Vec<(String, Ymap)>,
  modded: &Ymap,
) -> Result<(usize, String, YmapDiff), Box<dyn std::error::Error>> {
  let mut best: Option<(usize, String, YmapDiff)> = None;
  for (version, candidate) in candidates {
    let score = ymap_distance(&candidate, modded)?;
    log::info!("Candidate YMAP baseline {version}: {score} changes");
    if best.as_ref().is_none_or(|(best_score, _, _)| score < *best_score) {
      best = Some((score, version, YmapDiff::extract_from(&candidate, modded)));
    }
  }
  best.ok_or_else(|| "No usable vanilla YMAP candidates".into())
}
/// Information about a modded ymap file
#[derive(Debug, Clone)]
pub(super) struct ModYmapReference {
  pub(super) mod_name: String,
  pub(super) ymap_name: String,
  pub(super) mod_ymap_path: PathBuf,
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
    if let Some((mod_name, ymap_xml_name)) = file_name.split_once(ExtractYmap::FLATTEN_DELIMITER) {
      let ymap_name = ymap_xml_name.trim_end_matches(".xml");
      let info = ModYmapReference {
        mod_name: mod_name.to_string(),
        ymap_name: ymap_name.to_string(),
        mod_ymap_path: mod_file.clone(),
      };

      map.entry(ymap_xml_name.to_string()).or_default().push(info);
    } else {
      log::warn!(
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
  fn cached_ymap_diff_uses_best_baseline_and_preserves_latest_changes() {
    let xml: XmlYmap = quick_xml::de::from_str(include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/docs/sample/parent_refs/vanilla_parent.ymap.xml"
    )))
    .unwrap();
    let baseline: Ymap = xml.into();
    let entity_id = *baseline.entity_map.keys().next().unwrap();
    let mut modded = baseline.clone();
    modded.entity_map.get_mut(&entity_id).unwrap().position.x += 1.0;
    let mut latest = baseline.clone();
    latest.flags ^= 1;

    let (score, version, diff) =
      best_ymap_diff(vec![("latest".into(), latest.clone()), ("base".into(), baseline)], &modded)
        .unwrap();

    assert_eq!(version, "base");
    assert_eq!(score, 1);
    let merged = diff.apply_to(&latest, None);
    assert_eq!(merged.entity_map[&entity_id].position.x, modded.entity_map[&entity_id].position.x);
    assert_eq!(merged.flags, latest.flags);
  }

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
