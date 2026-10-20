use super::*;
use std::collections::HashMap;

impl IncrementalMerge<'_> {
  /// Merges `group` in its own working directory below the staging directory, named by its
  /// `index`, then moves the merged files into the staging directory's `ybn/` or `ymap/`;
  /// returns the group's duplicate report and the moved files.
  pub(super) fn merge_group(
    &self,
    index: usize,
    group: &Group,
    sources: &BTreeMap<String, Vec<&MergeSourceFile>>,
    baselines: &BTreeMap<String, &PathBuf>,
  ) -> Result<(DuplicateReport, Vec<PathBuf>)> {
    let vanilla = group
      .names
      .iter()
      .filter_map(|name| baselines.get(name).map(|path| (*path).clone()))
      .collect::<Vec<_>>();
    let sources = group
      .names
      .iter()
      .flat_map(|name| sources.get(name).into_iter().flatten().copied())
      .collect::<Vec<_>>();
    let mut duplicates = DuplicateReport {
      format_version: 1,
      files: BTreeMap::new(),
    };
    let work = self.staging.join(format!(".group-{index}"));
    let merged = if group.extension == "ybn" {
      MergeYbnConflicts {
        source_dir: self.source_cache.to_path_buf(),
        vanilla_dir: self.vanilla_cache.join("latest/ybn"),
        output_dir: work.clone(),
        omitted_files_path: work.join(".ybn_omit.txt"),
      }
      .run_with_latest_vanilla_files(
        None,
        &vanilla,
        Some(&sources.iter().map(|source| source.path.clone()).collect::<Vec<_>>()),
      )?;
      work.clone()
    } else {
      let vanilla_overwrites = group
        .names
        .iter()
        .filter_map(|name| Some((name.clone(), self.vanilla_overwrite(name)?.clone())))
        .collect::<HashMap<_, _>>();
      if sources.is_empty() && vanilla_overwrites.is_empty() {
        return Ok((duplicates, Vec::new()));
      }
      let directory = work.join("ymap");
      let ymap_sources = sources
        .iter()
        .map(|source| (source.resource.clone(), source.path.clone(), source.diff_path.clone()))
        .collect::<Vec<_>>();
      let overwrites = sources
        .iter()
        .filter_map(|source| Some((source.path.clone(), self.overwrite(source)?.clone())))
        .collect();
      MergeYmap {
        vanilla_dir: self.vanilla_cache.join("latest/ymap"),
        output_dir: directory.clone(),
        rebuild_all: true,
        blacklist_config: None,
        overwrites,
        vanilla_overwrites,
      }
      .run_with_latest_vanilla_files(&vanilla, &ymap_sources)?;
      duplicates = serde_json::from_reader(BufReader::new(fs::File::open(
        directory.join(".duplicates.json"),
      )?))?;
      directory
    };
    // Only the merged stream files are kept; the merge's own records stay behind.
    let target = self.staging.join(group.extension);
    let mut outputs = Vec::new();
    if merged.is_dir() {
      for entry in fs::read_dir(&merged)? {
        let path = entry?.path();
        let is_output = path.is_file()
          && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case(group.extension));
        if is_output {
          fs::create_dir_all(&target)?;
          let output = target.join(path.file_name().ok_or("Merged file name missing")?);
          fs::rename(&path, &output)?;
          outputs.push(output);
        }
      }
    }
    if work.exists() {
      fs::remove_dir_all(&work)?;
    }
    Ok((duplicates, outputs))
  }
}
