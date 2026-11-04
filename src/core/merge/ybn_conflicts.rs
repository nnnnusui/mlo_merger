//! Merging same-named YBN overrides from several resources against their latest vanilla file.

use std::{collections::BTreeMap, fs, io, path::PathBuf};

use crate::core::format::ybn::merge_ybn_deltas;

/// Merges same-named YBN resource overrides against vanilla Bounds data.
pub struct MergeYbnConflicts {
  /// Root the source paths are recorded relative to in [`Self::omitted_files_path`].
  pub source_dir: PathBuf,
  pub output_dir: PathBuf,
  /// Receives the merged sources' paths relative to [`Self::source_dir`], one per line.
  pub omitted_files_path: PathBuf,
}

impl MergeYbnConflicts {
  /// Merges each group of `source_files` sharing a file name into the vanilla file of that name
  /// among `vanilla_files`, writing it to the output directory; a group without one is skipped.
  pub fn run(
    &self,
    vanilla_files: &[PathBuf],
    source_files: &[PathBuf],
  ) -> Result<(), Box<dyn std::error::Error>> {
    crate::core::config::matching::with_config(|| self.run_configured(vanilla_files, source_files))?
  }

  fn run_configured(
    &self,
    vanilla_files: &[PathBuf],
    source_files: &[PathBuf],
  ) -> Result<(), Box<dyn std::error::Error>> {
    let groups = group_by_name(source_files);
    let vanilla = vanilla_by_name(vanilla_files)?;
    fs::create_dir_all(&self.output_dir)?;
    for entry in fs::read_dir(&self.output_dir)? {
      let path = entry?.path();
      if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("ybn")) {
        fs::remove_file(path)?;
      }
    }
    let mut omitted = Vec::new();
    let mut merged_groups = 0;
    for (name, paths) in &groups {
      let Some(vanilla_path) = vanilla.get(name) else {
        tracing::warn!("Skipping YBN source group {name}: latest vanilla file is missing");
        continue;
      };
      let vanilla_bytes = fs::read(vanilla_path)?;
      let mod_bytes = paths.iter().map(fs::read).collect::<io::Result<Vec<_>>>()?;
      let mod_refs = mod_bytes.iter().map(Vec::as_slice).collect::<Vec<_>>();
      let merged = merge_ybn_deltas(&vanilla_bytes, &mod_refs)?;
      let output_name = vanilla_path.file_name().ok_or("vanilla YBN filename missing")?;
      fs::write(self.output_dir.join(output_name), merged)?;
      for path in paths {
        omitted.push(path.strip_prefix(&self.source_dir)?.to_string_lossy().replace('\\', "/"));
      }
      merged_groups += 1;
      tracing::info!("    Merged YBN conflict {} from {} source files", name, paths.len());
    }
    omitted.sort();
    omitted.dedup();
    fs::write(&self.omitted_files_path, omitted.join("\n"))?;
    tracing::info!("Merged {merged_groups} vanilla-backed YBN source groups");
    Ok(())
  }
}

/// `paths` grouped by lowercase file name, each group sorted.
fn group_by_name(paths: &[PathBuf]) -> BTreeMap<String, Vec<PathBuf>> {
  let mut groups = BTreeMap::<String, Vec<PathBuf>>::new();
  for path in paths {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
      continue;
    };
    groups.entry(name.to_ascii_lowercase()).or_default().push(path.clone());
  }
  for paths in groups.values_mut() {
    paths.sort();
  }
  groups
}

/// The YBNs among `paths` keyed by lowercase file name; two of one name are an error.
fn vanilla_by_name(paths: &[PathBuf]) -> io::Result<BTreeMap<String, &PathBuf>> {
  let mut files = BTreeMap::new();
  for path in paths {
    if !path.is_file()
      || path.extension().is_none_or(|extension| !extension.eq_ignore_ascii_case("ybn"))
    {
      continue;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
      continue;
    };
    if files.insert(name.to_ascii_lowercase(), path).is_some() {
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
  fn groups_ybn_names_case_insensitively_and_keeps_hi_variants_separate() {
    let groups = group_by_name(&[
      PathBuf::from("resource_a/stream/sc1_18_0.ybn"),
      PathBuf::from("resource_b/stream/sub/SC1_18_0.YBN"),
      PathBuf::from("resource_a/stream/hi@sc1_18_0.ybn"),
      PathBuf::from("resource_b/stream/sub/hi@sc1_18_0.ybn"),
    ]);
    assert_eq!(groups["sc1_18_0.ybn"].len(), 2);
    assert_eq!(groups["hi@sc1_18_0.ybn"].len(), 2);
    assert_eq!(groups.len(), 2);
  }
}
