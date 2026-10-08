use super::*;

impl IncrementalMerge<'_> {
  pub(super) fn merge_group(
    &self,
    group: &Group,
    sources: &BTreeMap<String, Vec<&MergeSourceFile>>,
    baselines: &BTreeMap<String, &PathBuf>,
  ) -> Result<DuplicateReport> {
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
    if group.extension == "ybn" {
      let omit = self.staging.join(".ybn_omit.txt");
      MergeYbnConflicts {
        source_dir: self.source_cache.to_path_buf(),
        vanilla_dir: self.vanilla_cache.join("latest/ybn"),
        output_dir: self.staging.to_path_buf(),
        omitted_files_path: omit.clone(),
      }
      .run_with_latest_vanilla_files(
        None,
        &vanilla,
        Some(&sources.iter().map(|source| source.path.clone()).collect::<Vec<_>>()),
      )?;
      let _ = fs::remove_file(omit);
      for entry in fs::read_dir(self.staging)? {
        let path = entry?.path();
        if path.is_file()
          && path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("ybn"))
        {
          let directory = self.staging.join("ybn");
          fs::create_dir_all(&directory)?;
          fs::rename(&path, directory.join(path.file_name().ok_or("Merged file name missing")?))?;
        }
      }
    } else if !sources.is_empty() {
      let directory = self.staging.join("ymap");
      let sources = sources
        .iter()
        .map(|source| (source.resource.clone(), source.path.clone()))
        .collect::<Vec<_>>();
      MergeYmap {
        vanilla_dir: self.vanilla_cache.join("latest/ymap"),
        mod_dir: self.source_dir.to_path_buf(),
        mod_ymap_dir: self.source_dir.to_path_buf(),
        output_dir: directory.clone(),
        rebuild_all: true,
        blacklist_config: None,
      }
      .run_with_latest_vanilla_files(&vanilla, Some(&sources))?;
      let duplicate_path = directory.join(".duplicates.json");
      let duplicates: DuplicateReport =
        serde_json::from_reader(BufReader::new(fs::File::open(&duplicate_path)?))?;
      fs::remove_file(duplicate_path)?;
      let _ = fs::remove_file(directory.join("_copy_targets.txt"));
      let _ = fs::remove_file(directory.join("_managed_ymaps.txt"));
      let _ = fs::remove_dir_all(directory.join("clone"));
      return Ok(duplicates);
    }
    Ok(DuplicateReport {
      format_version: 1,
      files: BTreeMap::new(),
    })
  }
}
