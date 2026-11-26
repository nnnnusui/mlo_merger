use super::*;

impl BuildVanillaCache {
  /// Builds the requested vanilla-cache prefix and publishes it atomically, then brings its
  /// stream-file bounds index (`stream_file_rtree.json`) up to date.
  pub fn run(&self) -> Result<VanillaCacheResult> {
    let result = self.run_cache()?;
    crate::app::map::find::rtree::update_store_index(
      crate::app::map::find::rtree::Store::Vanilla,
      &self.output_dir,
    )?;
    Ok(result)
  }

  fn run_cache(&self) -> Result<VanillaCacheResult> {
    let vanilla_dir = fs::canonicalize(&self.vanilla_dir)?;
    let (manifest, revision) = crate::core::game_archive::load_content(
      &vanilla_dir,
      crate::core::game_archive::ArchiveContent::Map,
    )?;
    let through_index = resolve_through_version(&manifest, self.gamebuild.as_ref())?;
    let latest_version = manifest.versions[through_index].id.clone();
    let current = resolve_files(&manifest, through_index)?;
    // YMAPs and YBNs first shipped after the selected stage, of which the game may load an older
    // generation.
    let newer = resolve_files(&manifest, manifest.versions.len() - 1)?
      .into_iter()
      .filter(|(name, _)| {
        (name.ends_with(".ymap") || name.ends_with(".ybn")) && !current.contains_key(name)
      })
      .collect::<BTreeMap<_, _>>();
    let input_timestamps = source_timestamps(
      &vanilla_dir,
      &current.iter().chain(&newer).map(|(name, file)| (name.clone(), file.clone())).collect(),
    )?;
    let output_dir = prepare_output_path(&self.output_dir, &vanilla_dir)?;
    let previous = previous_manifest(&output_dir);
    let configured_additive =
      crate::core::config::dlc_activity::DlcActivityConfig::load(&crate::core::config::path())?
        .additive_archives;

    if !self.force
      && cache_is_current(
        &output_dir,
        previous.as_ref().map(|previous| &previous.manifest),
        &vanilla_dir,
        &revision,
        &latest_version,
        &input_timestamps,
        &configured_additive,
      )?
    {
      return Ok(VanillaCacheResult {
        latest_version,
        rebuilt: false,
        changes: StoreChanges::default(),
      });
    }

    let staging = Staging(create_staging_directory(&output_dir)?);
    let activity = super::activity::dlc_activity(
      &manifest,
      through_index,
      &current,
      &configured_additive,
      &revision,
    );
    if let Some(activity) = &activity {
      write_json(&staging.0.join(super::activity::DLC_ACTIVITY_FILE), activity)?;
    }
    let mut inactive = super::activity::inactive_files(activity.as_ref());
    for (name, (version, _)) in &newer {
      inactive.insert(name.clone(), format!("first shipped in {version}, after {latest_version}"));
    }
    let ybns = generation_sources(&vanilla_dir, &manifest, &current, &newer, ".ybn")?;
    let generations = write_generations(
      &staging.0,
      &output_dir,
      (&generation_sources(&vanilla_dir, &manifest, &current, &newer, ".ymap")?, &ybns),
      activity.as_ref(),
      &inactive,
      &newer.keys().cloned().collect(),
      &manifest.versions.iter().map(|version| version.id.clone()).collect::<Vec<_>>(),
      &revision,
      &latest_version,
    )?;
    super::ybn_repeats::write_repeats(&staging.0, &ybns, &inactive, &revision, &latest_version)?;
    // Only those an older generation stands for are diffed and carried; the others are added.
    let newer = newer
      .into_iter()
      .filter(|(name, _)| generations.successor(name).is_some())
      .collect::<BTreeMap<_, _>>();
    let latest_files = Self::build(
      &vanilla_dir,
      &staging.0,
      &output_dir,
      &latest_version,
      &revision,
      (&current, &newer),
      &input_timestamps,
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
    (current, newer): (&ResolvedFiles, &ResolvedFiles),
    input_timestamps: &BTreeMap<String, SourceTimestamp>,
  ) -> Result<BTreeMap<String, LatestFile>> {
    use rayon::prelude::*;
    let streams =
      current.iter().filter(|(name, _)| is_supported_stream_file(name)).collect::<Vec<_>>();
    tracing::info!("Reading {} vanilla stream files", streams.len());
    // Each YMAP is read whole for its parent, so they are read in parallel.
    let parents = streams
      .par_iter()
      .map(|(name, (_, file))| {
        let parent = || -> Result<Option<u32>> {
          let source = existing_artifact_path(vanilla_dir, file)?;
          let is_ymap = Path::new(name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ymap"));
          is_ymap.then(|| ymap_parent_hash(&source)).transpose()
        };
        parent().map_err(|error| format!("{name}: {error}"))
      })
      .collect::<std::result::Result<Vec<_>, String>>()?;
    let mut latest_files = BTreeMap::new();
    let mut parent_by_child = BTreeMap::new();
    for ((name, (version, file)), parent) in streams.into_iter().zip(parents) {
      if let Some(parent) = parent {
        parent_by_child.insert(name.to_ascii_lowercase(), parent);
      }
      latest_files.insert(name.clone(), latest_file(version, file));
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
        newer_files: newer
          .iter()
          .map(|(name, (version, file))| (name.clone(), latest_file(version, file)))
          .collect(),
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

/// Files keyed by lowercase name, each with the stage it comes from.
type ResolvedFiles = BTreeMap<String, (String, CachedFile)>;

fn latest_file(
  version: &str,
  file: &CachedFile,
) -> LatestFile {
  LatestFile {
    version: version.to_owned(),
    sha256: file.sha256.clone(),
    object: file.object.clone(),
    source: file.source.clone(),
  }
}

/// The YMAPs of `current` and `newer`, keyed by lowercase file name, with the first stage
/// shipping a YMAP of their name.
fn generation_sources(
  vanilla_dir: &Path,
  manifest: &VanillaCacheManifest,
  current: &BTreeMap<String, (String, CachedFile)>,
  newer: &BTreeMap<String, (String, CachedFile)>,
  extension: &str,
) -> Result<BTreeMap<String, generations::SourceYmap>> {
  let mut first_stages = BTreeMap::new();
  for (stage, version) in manifest.versions.iter().enumerate() {
    for name in version.changes.keys() {
      first_stages.entry(name.to_ascii_lowercase()).or_insert(stage);
    }
  }
  current
    .iter()
    .chain(newer)
    .filter(|(name, _)| name.ends_with(extension))
    .map(|(name, (_, file))| {
      let source = generations::SourceYmap {
        sha256: file.sha256.clone(),
        path: existing_artifact_path(vanilla_dir, file)?,
        first_stage: first_stages[name],
      };
      Ok((name.clone(), source))
    })
    .collect()
}
