use super::*;

impl BuildVanillaCache {
  /// Builds the requested vanilla-cache prefix and publishes it atomically, then brings its
  /// stream-file bounds index (`stream_file_rtree.json`) up to date.
  pub fn run(&self) -> Result<VanillaCacheResult> {
    let result = self.run_cache()?;
    crate::core::find::rtree::update_store_index(
      crate::core::find::rtree::Store::Vanilla,
      &self.output_dir,
    )?;
    Ok(result)
  }

  fn run_cache(&self) -> Result<VanillaCacheResult> {
    let vanilla_dir = fs::canonicalize(&self.vanilla_dir)?;
    let (manifest, revision) = crate::core::vanilla::load_manifest_with_revision(&vanilla_dir)?;
    let through_index = resolve_through_version(&manifest, self.gamebuild.as_ref())?;
    let latest_version = manifest.versions[through_index].id.clone();
    let current = resolve_files(&manifest, through_index)?;
    let input_timestamps = source_timestamps(&vanilla_dir, &current)?;
    let output_dir = prepare_output_path(&self.output_dir, &vanilla_dir)?;
    let previous = previous_manifest(&output_dir);

    if !self.force
      && cache_is_current(
        &output_dir,
        previous.as_ref().map(|previous| &previous.manifest),
        &vanilla_dir,
        &revision,
        &latest_version,
        &input_timestamps,
      )?
    {
      return Ok(VanillaCacheResult {
        latest_version,
        rebuilt: false,
        changes: StoreChanges::default(),
      });
    }

    let staging = Staging(create_staging_directory(&output_dir)?);
    let latest_files = Self::build(
      &vanilla_dir,
      &staging.0,
      &output_dir,
      &latest_version,
      &revision,
      &current,
      &input_timestamps,
    )?;
    write_generations(
      &staging.0,
      &output_dir,
      &generation_sources(&vanilla_dir, &manifest, through_index, &current)?,
      &manifest.versions.iter().map(|version| version.id.clone()).collect::<Vec<_>>(),
      &revision,
      &latest_version,
    )?;
    // YMAP version diffs are not generated until vanilla version inference is implemented;
    // see "Limitations" in docs/reference.md.
    // // Forced rebuilds take every diff again.
    // let previous_diffs = (!self.force).then_some(output_dir.as_path());
    // write_ymap_diffs(
    //   &vanilla_dir,
    //   &manifest,
    //   &revision,
    //   through_index,
    //   &staging.0,
    //   previous_diffs,
    // )?;
    publish_directory(&staging.0, &output_dir)?;
    let mut changes = StoreChanges::default();
    let mut previous_files: BTreeMap<_, _> = previous
      .iter()
      .flat_map(|previous| previous.manifest.files.values())
      .map(|file| (file.object.clone(), file.sha256.clone()))
      .collect();
    for file in latest_files.values() {
      let before = previous_files.remove(&file.object);
      changes.record(&file.object, before.as_deref(), Some(&file.sha256));
    }
    for (object, sha256) in previous_files {
      changes.record(object, Some(&sha256), None);
    }
    Ok(VanillaCacheResult {
      latest_version,
      rebuilt: true,
      changes,
    })
  }

  /// Writes the cache into `staging` and returns the latest files it records.
  fn build(
    vanilla_dir: &Path,
    staging: &Path,
    output_dir: &Path,
    latest_version: &str,
    revision: &str,
    current: &BTreeMap<String, (String, CachedFile)>,
    input_timestamps: &BTreeMap<String, SourceTimestamp>,
  ) -> Result<BTreeMap<String, LatestFile>> {
    let mut latest_files = BTreeMap::new();
    let mut parent_by_child = BTreeMap::new();
    for (name, (version, file)) in current {
      if !is_supported_stream_file(name) {
        continue;
      }
      let source = existing_artifact_path(vanilla_dir, file)?;
      if Path::new(name).extension().is_some_and(|extension| extension.eq_ignore_ascii_case("ymap"))
      {
        parent_by_child.insert(name.to_ascii_lowercase(), ymap_parent_hash(&source)?);
      }
      latest_files.insert(
        name.clone(),
        LatestFile {
          version: version.clone(),
          sha256: file.sha256.clone(),
          object: file.object.clone(),
          source: file.source.clone(),
        },
      );
    }

    write_json(
      &staging.join("cache_info.json"),
      &DerivedManifest {
        format_version: CACHE_SCHEMA,
        vanilla_dir: recorded_vanilla_dir(output_dir, vanilla_dir),
        vanilla_manifest_sha256: revision.to_owned(),
        latest_version: latest_version.to_owned(),
        vanilla_input_timestamps: input_timestamps.clone(),
        files: latest_files.clone(),
      },
    )?;
    write_json(
      &staging.join("ymap_relationships.json"),
      &YmapRelationshipIndex {
        format_version: CACHE_SCHEMA,
        vanilla_manifest_sha256: revision.to_owned(),
        version: latest_version.to_owned(),
        children_by_parent: children_by_parent(&parent_by_child)?,
      },
    )?;
    Ok(latest_files)
  }
}

/// The latest YMAPs of `current`, keyed by lowercase file name, with the first of the stages up
/// to `through_index` shipping a YMAP of their name.
fn generation_sources(
  vanilla_dir: &Path,
  manifest: &VanillaCacheManifest,
  through_index: usize,
  current: &BTreeMap<String, (String, CachedFile)>,
) -> Result<BTreeMap<String, generations::SourceYmap>> {
  let mut first_stages = BTreeMap::new();
  for (stage, version) in manifest.versions.iter().enumerate().take(through_index + 1) {
    for name in version.changes.keys() {
      first_stages.entry(name.to_ascii_lowercase()).or_insert(stage);
    }
  }
  current
    .iter()
    .filter(|(name, _)| name.ends_with(".ymap"))
    .map(|(name, (_, file))| {
      let source = generations::SourceYmap {
        sha256: file.sha256.clone(),
        path: existing_artifact_path(vanilla_dir, file)?,
        first_stage: first_stages.get(name).copied().unwrap_or(through_index),
      };
      Ok((name.clone(), source))
    })
    .collect()
}
