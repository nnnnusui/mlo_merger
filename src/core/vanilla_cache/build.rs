use super::*;

impl BuildVanillaCache {
  /// Builds the requested vanilla-cache prefix and publishes it atomically.
  pub fn run(&self) -> Result<()> {
    let vanilla_dir = self.vanilla_dir.canonicalize()?;
    let manifest_path = vanilla_dir.join("cache_info.json");
    let manifest_bytes = fs::read(&manifest_path)?;
    let manifest: VanillaCacheManifest =
      serde_json::from_reader(BufReader::new(manifest_bytes.as_slice()))?;
    if manifest.format_version != 1 {
      return Err(format!("Unsupported vanilla archive schema {}", manifest.format_version).into());
    }
    let through_index = resolve_through_version(&manifest, self.through_version.as_deref())?;
    let latest_version = manifest.versions[through_index].id.clone();
    let revision = format!("{:x}", Sha256::digest(&manifest_bytes));
    let current = resolve_files(&manifest, through_index)?;
    let input_timestamps = source_timestamps(&vanilla_dir, &current)?;
    let output_dir = prepare_output_path(&self.output_dir, &vanilla_dir)?;

    if !self.force
      && cache_is_current(&output_dir, &vanilla_dir, &revision, &latest_version, &input_timestamps)?
    {
      log::info!("Vanilla derived cache is current through {latest_version}");
      return Ok(());
    }

    let staging = Staging(create_staging_directory(&output_dir)?);
    Self::build(
      &vanilla_dir,
      &staging.0,
      &output_dir,
      &latest_version,
      &revision,
      &current,
      &input_timestamps,
    )?;
    publish_directory(&staging.0, &output_dir)?;
    log::info!("Built vanilla derived cache through {latest_version}");
    Ok(())
  }

  fn build(
    vanilla_dir: &Path,
    staging: &Path,
    output_dir: &Path,
    latest_version: &str,
    revision: &str,
    current: &BTreeMap<String, (String, CachedFile)>,
    input_timestamps: &BTreeMap<String, SourceTimestamp>,
  ) -> Result<()> {
    let latest_dir = staging.join("latest");
    fs::create_dir_all(latest_dir.join("ymap"))?;
    fs::create_dir_all(latest_dir.join("ybn"))?;
    let mut latest_files = BTreeMap::new();
    let mut parent_by_child = BTreeMap::new();
    for (name, (version, file)) in current {
      let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
      if extension != "ymap" && extension != "ybn" {
        continue;
      }
      let source = existing_artifact_path(vanilla_dir, file)?;
      if extension == "ymap" {
        parent_by_child.insert(name.to_ascii_lowercase(), ymap_parent_hash(&source)?);
      }
      let destination_relative = format!("latest/{extension}/{name}");
      let destination = staging.join(&destination_relative);
      let final_destination = output_dir.join(&destination_relative);
      let link_target = relative_link_target(
        final_destination.parent().ok_or("Latest output has no parent")?,
        &source,
      );
      create_file_symlink(&link_target, &destination)?;
      latest_files.insert(
        name.clone(),
        LatestFile {
          version: version.clone(),
          sha256: file.sha256.clone(),
          object: destination_relative,
          vanilla_object: file.object.clone(),
          source: file.source.clone(),
        },
      );
    }

    write_json(
      &staging.join("cache_info.json"),
      &DerivedManifest {
        format_version: CACHE_SCHEMA,
        vanilla_manifest_sha256: revision.to_owned(),
        latest_version: latest_version.to_owned(),
        vanilla_input_timestamps: input_timestamps.clone(),
        files: latest_files,
      },
    )?;
    write_json(
      &staging.join("ymap_relationships.json"),
      &YmapRelationshipIndex {
        format_version: CACHE_SCHEMA,
        vanilla_manifest_sha256: revision.to_owned(),
        version: latest_version.to_owned(),
        children_by_parent_hash: children_by_parent_hash(&parent_by_child),
      },
    )?;
    Ok(())
  }
}
