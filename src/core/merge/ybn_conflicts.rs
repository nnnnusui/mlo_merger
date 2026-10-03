use std::{
  collections::BTreeMap,
  fs, io,
  path::{Path, PathBuf},
};

use walkdir::WalkDir;

use crate::core::{
  codewalker::CodeWalker,
  extract::get_resource_directories,
  format::gamefile::ybn::{merge_ybn_deltas, ybn_to_xml},
};

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

  /// Writes merged YBNs for source basename conflicts and records their source paths for omission.
  pub fn run(
    &self,
    codewalker: Option<&CodeWalker>,
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
    let vanilla = collect_vanilla_ybns(&self.vanilla_dir)?;
    for (name, _) in &conflicts {
      if !vanilla.contains_key(name) {
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
      let vanilla_path = &vanilla[name];
      let vanilla_bytes = fs::read(vanilla_path)?;
      let mod_bytes = paths.iter().map(fs::read).collect::<io::Result<Vec<_>>>()?;
      let mod_refs = mod_bytes.iter().map(Vec::as_slice).collect::<Vec<_>>();
      let merged = merge_ybn_deltas(&vanilla_bytes, &mod_refs)?;
      let output_name = vanilla_path.file_name().ok_or("vanilla YBN filename missing")?;
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
}
