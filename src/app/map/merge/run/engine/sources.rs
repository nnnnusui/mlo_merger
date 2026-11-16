//! Grouping and registering the sources, and the changes carried to other YMAPs.

use super::*;

impl<'a> MergeRun<'a> {
  /// The sources grouped by file name, leaving out those whose file or latest vanilla YMAP is
  /// missing, and a resource's second YMAP of one name.
  pub(super) fn group_sources(
    &mut self,
    raw_sources: &[SourceYmap],
  ) -> Result<SourcesByName, Box<dyn std::error::Error>> {
    let mut by_name = SourcesByName::new();
    for (resource, path, diff_path) in raw_sources {
      let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Source YMAP filename is not UTF-8")?;
      by_name.entry(name.to_string()).or_default().push(ModYmapReference {
        mod_name: resource.clone(),
        ymap_name: name.to_string(),
        mod_ymap_path: path.clone(),
        diff_path: diff_path.clone(),
      });
    }
    let vanilla_map_paths = &self.vanilla_map_paths;
    let registered_maps = &mut self.registered_maps;
    by_name.retain(|_, references| {
      references.retain(|reference| {
        if !fs::is_file(&reference.mod_ymap_path) {
          tracing::warn!(
            "Source YMAP is missing for resource {}; skipping {}",
            reference.mod_name,
            reference.mod_ymap_path.display()
          );
          return false;
        }
        let name = Path::new(&reference.ymap_name).file_stem().and_then(|name| name.to_str());
        let Some(name) = name else {
          tracing::warn!(
            "Source YMAP has an invalid filename: {}",
            reference.mod_ymap_path.display()
          );
          return false;
        };
        let hash = reference_hash(name);
        if !vanilla_map_paths.contains_key(&hash) {
          tracing::warn!(
            "Latest vanilla YMAP {name}.ymap is missing for resource {}; skipping {}",
            reference.mod_name,
            reference.mod_ymap_path.display()
          );
          return false;
        }
        if !registered_maps.insert((reference.mod_name.clone(), hash)) {
          tracing::warn!(
            "Duplicate original map {name} within resource {}; keeping the first and skipping {}",
            reference.mod_name,
            reference.mod_ymap_path.display()
          );
          return false;
        }
        true
      });
      !references.is_empty()
    });
    tracing::info!("Found {} unique YMAP files across mods", by_name.len());
    Ok(by_name)
  }

  /// The vanilla overwrites by name hash, adding to `by_name` the YMAPs they change that no
  /// source does.
  pub(super) fn vanilla_overwrites(
    &self,
    by_name: &mut SourcesByName,
  ) -> Result<HashMap<u32, &'a VanillaOverwrite>, Box<dyn std::error::Error>> {
    let mut vanilla_overwrites = HashMap::new();
    let merge: &'a MergeYmap = self.merge;
    for (name, overwrite) in &merge.vanilla_overwrites {
      let hash = reference_hash(name.trim_end_matches(".ymap"));
      if !self.vanilla_map_paths.contains_key(&hash) {
        tracing::debug!("Vanilla overwrite of {name} names no latest vanilla YMAP; not applied");
        continue;
      }
      vanilla_overwrites.insert(hash, overwrite);
    }
    let mut overwritten = vanilla_overwrites.keys().copied().collect::<HashSet<_>>();
    for name in by_name.keys() {
      overwritten.remove(&reference_hash(name.trim_end_matches(".ymap")));
    }
    for hash in overwritten {
      by_name.entry(self.vanilla_name(hash)?.to_string()).or_default();
    }
    Ok(vanilla_overwrites)
  }

  /// The file name of the latest vanilla YMAP with name hash `hash`.
  pub(super) fn vanilla_name(
    &self,
    hash: u32,
  ) -> Result<&str, Box<dyn std::error::Error>> {
    Ok(
      self.vanilla_map_paths[&hash]
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("non-UTF8 vanilla filename")?,
    )
  }

  /// Registers the latest vanilla YMAPs and the sources, and the entities of every version of
  /// each source's YMAP for parent lookups.
  pub(super) fn register_sources(
    &mut self,
    by_name: &SourcesByName,
  ) -> Result<(), Box<dyn std::error::Error>> {
    for path in self.latest_files {
      let name = path.file_name().ok_or("Vanilla YMAP filename missing")?;
      let name = name.to_str().ok_or("non-UTF8 vanilla filename")?;
      self.sources.register(None, name, path.clone())?;
    }
    tracing::info!("Registered {} vanilla YMAP files for merge", self.latest_files.len());
    for references in by_name.values() {
      for reference in references {
        self.sources.register(
          Some(&reference.mod_name),
          &reference.ymap_name,
          reference.mod_ymap_path.clone(),
        )?;
      }
    }
    // Parent indices into a YMAP a resource adds point into the one its resource ships.
    for (name, versions) in by_name {
      if let [single] = versions.as_slice()
        && self.merge.added.contains(&name.to_ascii_lowercase())
      {
        self.sources.set_original(reference_hash(name.trim_end_matches(".ymap")), &single.mod_name);
      }
    }
    for (name, versions) in by_name {
      let hash = reference_hash(name.trim_end_matches(".ymap"));
      if let Some(original) = self.sources.get(None, hash)? {
        self.references.register_entities(hash, &original.entities);
      }
      for version in versions {
        if let Some(original) = self.sources.get(Some(&version.mod_name), hash)? {
          self.references.register_entities(hash, &original.entities);
        }
      }
    }
    Ok(())
  }

  /// The changes carried to YMAPs other than the sources' own: the LOD light removals a source
  /// makes only in the other YMAP of a pair, and the changes carried over to a newer
  /// generation. Adds to `by_name` the YMAPs they change.
  pub(super) fn carried_diffs(
    &mut self,
    by_name: &mut SourcesByName,
  ) -> Result<(ImpliedDiffs, PropagatedDiffs), Box<dyn std::error::Error>> {
    let implied = self.merge.implied_diffs(
      by_name,
      &self.vanilla_map_paths,
      &mut self.sources,
      &mut self.references,
      &self.registered_maps,
    )?;
    let propagated = self.merge.propagated_diffs(&self.vanilla_map_paths)?;
    for hash in implied.keys().chain(propagated.keys()) {
      let merged =
        by_name.keys().any(|name| reference_hash(name.trim_end_matches(".ymap")) == *hash);
      if merged {
        continue;
      }
      let name = self.vanilla_name(*hash)?.to_string();
      if let Some(original) = self.sources.get(None, *hash)? {
        self.references.register_entities(*hash, &original.entities);
      }
      by_name.entry(name).or_default();
    }
    Ok((implied, propagated))
  }
}
