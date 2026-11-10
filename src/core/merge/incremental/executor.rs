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
    let mut vanilla = group
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
    // A YMAP resources add is merged against an empty YMAP.
    let plans =
      group.names.iter().filter_map(|name| Some((name, self.added.get(name)?))).collect::<Vec<_>>();
    for (name, _) in &plans {
      let path = work.join("added").join(name);
      fs::create_dir_all(path.parent().ok_or("Baseline path has no parent")?)?;
      fs::write(&path, crate::core::source_cache::empty_baseline_bytes()?)?;
      vanilla.push(path);
    }
    let merged = if group.extension == "ybn" {
      MergeYbnConflicts {
        source_dir: self.source_cache.to_path_buf(),
        output_dir: work.clone(),
        omitted_files_path: work.join(".ybn_omit.txt"),
      }
      .run(&vanilla, &sources.iter().map(|source| source.path.clone()).collect::<Vec<_>>())?;
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
      let (propagated, stream_files): (Vec<&MergeSourceFile>, Vec<&MergeSourceFile>) =
        sources.iter().copied().partition(|source| source.propagated);
      // The versions of a YMAP resources add are merged in the order the overwrite cache
      // records, the one loaded last first.
      let mut stream_files = stream_files;
      stream_files.sort_by_key(|source| {
        self
          .added
          .get(&source.file_name.to_ascii_lowercase())
          .and_then(|file| file.merge_position(&source.resource))
          .unwrap_or(0)
      });
      let ymap_sources = stream_files
        .iter()
        .map(|source| (source.resource.clone(), source.path.clone(), source.diff_path.clone()))
        .collect::<Vec<_>>();
      let overwrites = stream_files
        .iter()
        .filter_map(|source| Some((source.path.clone(), self.overwrite(source)?.clone())))
        .collect();
      MergeYmap {
        vanilla_dir: self.vanilla_cache.join("latest/ymap"),
        output_dir: directory.clone(),
        rebuild_all: true,
        overwrites,
        vanilla_overwrites,
        propagated: propagated
          .iter()
          .map(|source| (source.resource.clone(), source.file_name.clone(), source.path.clone()))
          .collect(),
        added: plans.iter().map(|(name, _)| (*name).clone()).collect(),
        replacing: plans
          .iter()
          .filter(|(_, file)| file.replaces_others())
          .map(|(name, _)| (*name).clone())
          .collect(),
      }
      .run_with_latest_vanilla_files(&vanilla, &ymap_sources)?;
      duplicates = crate::common::json::read_json(directory.join(".duplicates.json"))?;
      // The report names each file relative to its store, as the merge metadata does.
      let stored = vanilla
        .iter()
        .map(|path| (path.clone(), self.stored_vanilla_path(path)))
        .chain(
          sources.iter().map(|source| (source.path.clone(), PathBuf::from(&source.stored_path))),
        )
        .collect::<HashMap<_, _>>();
      for duplicate in duplicates.files.values_mut().flatten() {
        let contributions = std::iter::once(&mut duplicate.applied)
          .chain(duplicate.ignored.iter_mut().map(|ignored| &mut ignored.source));
        for contribution in contributions {
          if let Some(path) = stored.get(&contribution.path) {
            contribution.path = path.clone();
          }
        }
      }
      directory
    };
    // Only the merged stream files are kept; the merge's own records stay behind.
    let target = self.staging.join(group.extension);
    let mut outputs = Vec::new();
    if fs::is_dir(&merged) {
      for entry in fs::read_dir(&merged)? {
        let path = entry?.path();
        let is_output = fs::is_file(&path)
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
    if fs::exists(&work).unwrap_or(false) {
      fs::remove_dir_all(&work)?;
    }
    Ok((duplicates, outputs))
  }
}
