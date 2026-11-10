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
    // A YMAP resources add is merged against an empty YMAP, or against its base source's when it
    // has additional sources.
    let plans =
      group.names.iter().filter_map(|name| Some((name, self.added.get(name)?))).collect::<Vec<_>>();
    for (name, plan) in &plans {
      if plan.additional.is_empty() {
        let path = work.join("added").join(name);
        fs::create_dir_all(path.parent().ok_or("Baseline path has no parent")?)?;
        fs::write(&path, crate::core::source_cache::empty_baseline_bytes()?)?;
        vanilla.push(path);
      } else {
        let base = sources
          .iter()
          .find(|source| {
            source.resource == plan.base && source.file_name.eq_ignore_ascii_case(name)
          })
          .ok_or_else(|| format!("Base source of added YMAP {name} is missing"))?;
        vanilla.push(base.path.clone());
      }
    }
    let is_base_with_additional = |source: &MergeSourceFile| {
      self
        .added
        .get(&source.file_name.to_ascii_lowercase())
        .is_some_and(|plan| !plan.additional.is_empty() && plan.base == source.resource)
    };
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
      // A base source with additional sources is merged through its overwrite as removals from
      // its own YMAP, which the overwrite cache holds; its overwrite is applied there already.
      let ymap_sources = stream_files
        .iter()
        .map(|source| {
          let diff_path = if is_base_with_additional(source) {
            self
              .added
              .get(&source.file_name.to_ascii_lowercase())
              .and_then(|plan| plan.base_diff.clone())
          } else {
            source.diff_path.clone()
          };
          (source.resource.clone(), source.path.clone(), diff_path)
        })
        .collect::<Vec<_>>();
      let overwrites = stream_files
        .iter()
        .filter(|source| !is_base_with_additional(source))
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
        added: plans
          .iter()
          .filter(|(_, plan)| plan.additional.is_empty())
          .map(|(name, _)| (*name).clone())
          .collect(),
        replacing: plans
          .iter()
          .filter(|(_, plan)| plan.replacing)
          .map(|(name, _)| (*name).clone())
          .collect(),
      }
      .run_with_latest_vanilla_files(&vanilla, &ymap_sources)?;
      duplicates = crate::common::json::read_json(directory.join(".duplicates.json"))?;
      // The report names each file relative to its store, as the merge metadata does.
      // A base source merged as the baseline is named as the source it is.
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
