use std::{
  collections::BTreeMap,
  fs, io,
  path::{Path, PathBuf},
};

use walkdir::WalkDir;

use crate::core::{
  codewalker::CodeWalker,
  extract::get_resource_directories,
  format::ybn::{diff::YbnDiff, merge_ybn_deltas, read_ybn, write_ybn, ybn_to_xml},
};

use super::cache_inputs::VanillaHistory;

/// Merges same-named YBN resource overrides against vanilla Bounds data.
pub struct MergeYbnConflicts {
  pub source_dir: PathBuf,
  pub vanilla_dir: PathBuf,
  pub output_dir: PathBuf,
  pub omitted_files_path: PathBuf,
}

impl MergeYbnConflicts {
  /// Validates vanilla coverage and reports whether YBN conflicts require merging.
  pub fn has_conflicts(
    source_dir: &Path,
    vanilla_dir: &Path,
  ) -> io::Result<bool> {
    let conflicts = collect_ybn_groups(source_dir)?
      .into_iter()
      .filter(|(_, paths)| paths.len() > 1)
      .collect::<Vec<_>>();
    if conflicts.is_empty() {
      return Ok(false);
    }
    let vanilla = collect_vanilla_ybns(vanilla_dir)?;
    for (name, _) in &conflicts {
      if !vanilla.contains_key(name) {
        return Err(io::Error::new(
          io::ErrorKind::NotFound,
          format!("YBN conflict {name} has no vanilla baseline in {}", vanilla_dir.display()),
        ));
      }
    }
    Ok(!conflicts.is_empty())
  }

  /// Checks conflict coverage against the latest raw YBN files in vanilla history.
  pub fn has_conflicts_with_vanilla_cache(
    source_dir: &Path,
    history: &VanillaHistory,
  ) -> io::Result<bool> {
    let conflicts = collect_ybn_groups(source_dir)?
      .into_iter()
      .filter(|(_, paths)| paths.len() > 1)
      .collect::<Vec<_>>();
    for (name, _) in &conflicts {
      history.latest_file(name).map_err(|error| io::Error::other(error.to_string()))?;
    }
    Ok(!conflicts.is_empty())
  }

  /// Writes merged YBNs for source basename conflicts and records their source paths for omission.
  pub fn run(
    &self,
    codewalker: Option<&CodeWalker>,
  ) -> Result<(), Box<dyn std::error::Error>> {
    self.run_inner(codewalker, None)
  }

  /// Merges raw source YBNs against the latest cache state after scoring history candidates.
  pub fn run_with_vanilla_cache(
    &self,
    codewalker: Option<&CodeWalker>,
    history: &VanillaHistory,
  ) -> Result<(), Box<dyn std::error::Error>> {
    self.run_inner(codewalker, Some(history))
  }

  fn run_inner(
    &self,
    codewalker: Option<&CodeWalker>,
    history: Option<&VanillaHistory>,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let groups = collect_ybn_groups(&self.source_dir)?;
    let conflicts = groups.into_iter().filter(|(_, paths)| paths.len() > 1).collect::<Vec<_>>();
    if conflicts.is_empty() {
      fs::create_dir_all(&self.output_dir)?;
      for entry in fs::read_dir(&self.output_dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("ybn")) {
          fs::remove_file(path)?;
        }
      }
      fs::write(&self.omitted_files_path, "")?;
      log::info!("No conflicting YBN basenames found");
      return Ok(());
    }
    let vanilla =
      if history.is_none() { Some(collect_vanilla_ybns(&self.vanilla_dir)?) } else { None };
    for (name, _) in &conflicts {
      if let Some(history) = history {
        history.latest_file(name)?;
      } else if !vanilla.as_ref().unwrap().contains_key(name) {
        return Err(
          format!("YBN conflict {name} has no vanilla baseline in {}", self.vanilla_dir.display())
            .into(),
        );
      }
    }
    fs::create_dir_all(&self.output_dir)?;
    for entry in fs::read_dir(&self.output_dir)? {
      let path = entry?.path();
      if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("ybn")) {
        fs::remove_file(path)?;
      }
    }
    let mut omitted = Vec::new();
    let temporary = std::env::temp_dir().join(format!("mlo_ybn_merge_{}", std::process::id()));
    fs::create_dir_all(&temporary)?;
    for (name, paths) in &conflicts {
      let (vanilla_bytes, output_name) = if let Some(history) = history {
        let latest = history.latest_file(name)?;
        let output_name =
          Path::new(&latest.object).file_name().ok_or("vanilla YBN filename missing")?;
        (history.ybn_bytes(latest)?, output_name.to_os_string())
      } else {
        let vanilla_path = &vanilla.as_ref().unwrap()[name];
        (
          fs::read(vanilla_path)?,
          vanilla_path.file_name().ok_or("vanilla YBN filename missing")?.to_os_string(),
        )
      };
      let mod_bytes = if let Some(history) = history {
        paths
          .iter()
          .map(|path| {
            let source_bytes = fs::read(path)?;
            let source_model = read_ybn(&source_bytes)?;
            let (difference_count, version, diff) = best_ybn_diff(history, name, &source_model)?;
            log::info!("Best YBN baseline for {}: {version} ({difference_count} changes)", path.display());
            let latest = history.latest_file(name)?;
            match diff.apply_rebased_to(&history.ybn_model(latest)?) {
              Ok(rebased) => Ok(write_ybn(&rebased)?),
              Err(error) => {
                log::warn!("Cannot semantically rebase YBN changes for {}; using the source file directly with the native geometry merger: {error}", path.display());
                Ok(source_bytes)
              }
            }
          })
          .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?
      } else {
        paths.iter().map(fs::read).collect::<io::Result<Vec<_>>>()?
      };
      let mod_refs = mod_bytes.iter().map(Vec::as_slice).collect::<Vec<_>>();
      let merged = merge_ybn_deltas(&vanilla_bytes, &mod_refs)?;
      let output = self.output_dir.join(output_name);
      if let Some(codewalker) = codewalker {
        let xml = ybn_to_xml(&merged)?;
        let xml_path = temporary.join(format!("{name}.xml"));
        fs::write(&xml_path, &xml)?;
        codewalker.game_file_from_xml(&xml_path, &output)?;
      } else {
        fs::write(output, merged)?;
      }
      for path in paths {
        omitted.push(path.strip_prefix(&self.source_dir)?.to_string_lossy().replace('\\', "/"));
      }
      log::info!("    Merged YBN conflict {} from {} source files", name, paths.len());
    }
    let _ = fs::remove_dir_all(temporary);
    omitted.sort();
    omitted.dedup();
    fs::write(&self.omitted_files_path, omitted.join("\n"))?;
    log::info!("Merged {} conflicting YBN basenames", conflicts.len());
    Ok(())
  }
}

fn collect_ybn_groups(source_dir: &Path) -> io::Result<BTreeMap<String, Vec<PathBuf>>> {
  let mut groups = BTreeMap::<String, Vec<PathBuf>>::new();
  let resources = get_resource_directories(&source_dir.to_path_buf())
    .map_err(|error| io::Error::other(error.to_string()))?;
  for resource in resources {
    for entry in WalkDir::new(resource).follow_links(false) {
      let entry = entry.map_err(|error| io::Error::other(error.to_string()))?;
      if !entry.file_type().is_file()
        || entry.path().extension().is_none_or(|extension| !extension.eq_ignore_ascii_case("ybn"))
      {
        continue;
      }
      let Some(name) = entry.path().file_name().and_then(|name| name.to_str()) else {
        continue;
      };
      groups.entry(name.to_ascii_lowercase()).or_default().push(entry.into_path());
    }
  }
  for paths in groups.values_mut() {
    paths.sort();
  }
  Ok(groups)
}

fn best_ybn_diff(
  history: &VanillaHistory,
  name: &str,
  source: &crate::core::format::ybn::model::Bound,
) -> Result<(usize, String, YbnDiff), Box<dyn std::error::Error>> {
  let mut best: Option<(usize, String, YbnDiff)> = None;
  for (version, candidate) in history.candidate_files(name)? {
    let candidate_model = history.ybn_model(&candidate)?;
    let Ok(diff) = YbnDiff::extract_from(&candidate_model, source) else {
      continue;
    };
    let difference_count = diff.bound_diffs.len() + diff.polygon_diffs.len();
    if best.as_ref().is_none_or(|(best_count, _, _)| difference_count < *best_count) {
      best = Some((difference_count, version, diff));
    }
  }
  best.ok_or_else(|| format!("No supported YBN baseline diff for {name}").into())
}

fn collect_vanilla_ybns(vanilla_dir: &Path) -> io::Result<BTreeMap<String, PathBuf>> {
  let mut files = BTreeMap::new();
  for entry in WalkDir::new(vanilla_dir).follow_links(false) {
    let entry = entry.map_err(|error| io::Error::other(error.to_string()))?;
    if !entry.file_type().is_file()
      || entry.path().extension().is_none_or(|extension| !extension.eq_ignore_ascii_case("ybn"))
    {
      continue;
    }
    let Some(name) = entry.path().file_name().and_then(|name| name.to_str()).map(str::to_string)
    else {
      continue;
    };
    let key = name.to_ascii_lowercase();
    if files.insert(key, entry.into_path()).is_some() {
      return Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("duplicate vanilla YBN basename: {name}"),
      ));
    }
  }
  Ok(files)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::{
    format::ybn::{diff::YbnDiff, read_ybn, xml::xml_to_ybn},
    vanilla::{CacheVersion, CachedFile, FileChange, VanillaCacheManifest, write_json},
  };
  use sha2::{Digest, Sha256};
  use std::collections::BTreeMap;

  #[test]
  fn groups_ybn_basename_conflicts_case_insensitively_and_keeps_hi_variant_separate() {
    let temp = std::env::temp_dir().join(format!("mlo_ybn_grouping_{}", std::process::id()));
    let first = temp.join("resource_a/stream");
    let second = temp.join("resource_b/stream/sub");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    fs::write(temp.join("resource_a/fxmanifest.lua"), []).unwrap();
    fs::write(temp.join("resource_b/fxmanifest.lua"), []).unwrap();
    fs::write(first.join("sc1_18_0.ybn"), []).unwrap();
    fs::write(second.join("SC1_18_0.YBN"), []).unwrap();
    fs::write(first.join("hi@sc1_18_0.ybn"), []).unwrap();
    fs::write(second.join("hi@sc1_18_0.ybn"), []).unwrap();

    let groups = collect_ybn_groups(&temp).unwrap();
    assert_eq!(groups["sc1_18_0.ybn"].len(), 2);
    assert_eq!(groups["hi@sc1_18_0.ybn"].len(), 2);
    assert_eq!(groups.len(), 2);
    fs::remove_dir_all(temp).unwrap();
  }

  #[test]
  fn requires_vanilla_only_when_a_basename_conflict_exists() {
    let temp = std::env::temp_dir().join(format!("mlo_ybn_baseline_{}", std::process::id()));
    let source = temp.join("source");
    let stream_a = source.join("resource_a/stream");
    let stream_b = source.join("resource_b/stream");
    fs::create_dir_all(&stream_a).unwrap();
    fs::create_dir_all(&stream_b).unwrap();
    fs::write(source.join("resource_a/fxmanifest.lua"), []).unwrap();
    fs::write(source.join("resource_b/fxmanifest.lua"), []).unwrap();
    assert!(!MergeYbnConflicts::has_conflicts(&source, &temp.join("missing_vanilla")).unwrap());

    fs::write(stream_a.join("collision.ybn"), []).unwrap();
    fs::write(stream_b.join("collision.ybn"), []).unwrap();
    assert!(MergeYbnConflicts::has_conflicts(&source, &temp.join("missing_vanilla")).is_err());
    let vanilla = temp.join("vanilla");
    fs::create_dir_all(&vanilla).unwrap();
    fs::write(vanilla.join("collision.ybn"), []).unwrap();
    assert!(MergeYbnConflicts::has_conflicts(&source, &vanilla).unwrap());
    fs::remove_dir_all(temp).unwrap();
  }

  #[test]
  fn cache_merge_selects_least_changed_ybn_then_merges_onto_latest() {
    let temp = std::env::temp_dir().join(format!("mlo_ybn_cache_merge_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    let source = temp.join("source");
    let cache = temp.join("vanilla-cache");
    let output = temp.join("merged");
    let source_a = source.join("resource_a/stream");
    let source_b = source.join("resource_b/stream");
    fs::create_dir_all(&source_a).unwrap();
    fs::create_dir_all(&source_b).unwrap();
    fs::write(source.join("resource_a/fxmanifest.lua"), []).unwrap();
    fs::write(source.join("resource_b/fxmanifest.lua"), []).unwrap();

    let base_xml = include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/docs/sample/ybn_conflicts/geometry_bvh.ybn.xml"
    ));
    let add_polygon = |xml: &str, vertices: [u8; 3]| {
      xml.replacen(
        "    </Polygons>",
        &format!(
          "     <Triangle m=\"0\" v1=\"{}\" v2=\"{}\" v3=\"{}\" f1=\"0\" f2=\"0\" f3=\"0\" />\n    </Polygons>",
          vertices[0], vertices[1], vertices[2]
        ),
        1,
      )
    };
    let patch_xml = add_polygon(base_xml, [0, 5, 9]);
    let latest_xml = add_polygon(&patch_xml, [1, 8, 12]);
    let mod_a_xml = add_polygon(&patch_xml, [2, 6, 11]);
    let mod_b_xml = add_polygon(&patch_xml, [3, 4, 14]);
    let binaries =
      [base_xml, patch_xml.as_str(), latest_xml.as_str()].map(|xml| xml_to_ybn(xml).unwrap());
    let source_binaries = [xml_to_ybn(&mod_a_xml).unwrap(), xml_to_ybn(&mod_b_xml).unwrap()];
    let hash = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
    let mut changes: Vec<CacheVersion> = Vec::new();
    let mut previous_sha = None;
    for (index, (id, bytes)) in
      [("0000-base", &binaries[0]), ("0001-patch", &binaries[1]), ("0002-latest", &binaries[2])]
        .into_iter()
        .enumerate()
    {
      let object = format!("{id}/ybn/collision.ybn");
      let path = cache.join(&object);
      fs::create_dir_all(path.parent().unwrap()).unwrap();
      fs::write(&path, bytes).unwrap();
      let sha256 = hash(bytes);
      changes.push(CacheVersion {
        id: id.into(),
        parent: index.checked_sub(1).map(|parent| changes[parent].id.clone()),
        archives: vec![],
        changes: BTreeMap::from([(
          "collision.ybn".into(),
          FileChange {
            previous_sha256: previous_sha.clone(),
            file: CachedFile {
              sha256: sha256.clone(),
              object,
              source: format!("fixture.rpf/{id}/collision.ybn"),
            },
          },
        )]),
        unchanged: 0,
      });
      previous_sha = Some(sha256);
    }
    let manifest = VanillaCacheManifest {
      format_version: 1,
      game_dir: temp.join("missing-game"),
      versions: changes,
    };
    write_json(&cache.join("cache_info.json"), &manifest).unwrap();

    for (directory, bytes) in [(&source_a, &source_binaries[0]), (&source_b, &source_binaries[1])] {
      fs::write(directory.join("collision.ybn"), bytes).unwrap();
    }
    let history = VanillaHistory::load(&cache).unwrap();
    let source_model = read_ybn(&source_binaries[0]).unwrap();
    let (score, version, diff) = best_ybn_diff(&history, "collision.ybn", &source_model).unwrap();
    assert_eq!(version, "0001-patch");
    assert_eq!(score, 1);
    assert_eq!(diff.polygon_diffs.len(), 1);
    let latest_model = read_ybn(&binaries[2]).unwrap();
    let rebased = diff.apply_rebased_to(&latest_model).unwrap();
    let rebased_diff = YbnDiff::extract_from(&latest_model, &rebased).unwrap();
    assert_eq!(rebased_diff.polygon_diffs.len(), 1);

    let merger = MergeYbnConflicts {
      source_dir: source,
      vanilla_dir: temp.join("unused-vanilla"),
      output_dir: output.clone(),
      omitted_files_path: temp.join("omitted.txt"),
    };
    assert!(
      MergeYbnConflicts::has_conflicts_with_vanilla_cache(&merger.source_dir, &history).unwrap()
    );
    merger.run_with_vanilla_cache(None, &history).unwrap();
    let merged = read_ybn(&fs::read(output.join("collision.ybn")).unwrap()).unwrap();
    let latest = read_ybn(&binaries[2]).unwrap();
    let merged_diff = YbnDiff::extract_from(&latest, &merged).unwrap();
    assert_eq!(merged_diff.polygon_diffs.len(), 2);
    assert_eq!(fs::read_to_string(temp.join("omitted.txt")).unwrap().lines().count(), 2);
    fs::remove_dir_all(temp).unwrap();
  }
}
