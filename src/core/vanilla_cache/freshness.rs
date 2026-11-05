use super::*;

/// Reads the manifest the cache was last published with, if it is readable.
pub(super) fn previous_manifest(output_dir: &Path) -> Option<DerivedManifest> {
  crate::common::json::read_json(output_dir.join("cache_info.json")).ok()
}

pub(super) fn cache_is_current(
  output_dir: &Path,
  manifest: Option<&DerivedManifest>,
  vanilla_dir: &Path,
  revision: &str,
  latest_version: &str,
  input_timestamps: &BTreeMap<String, SourceTimestamp>,
) -> Result<bool> {
  let relationships_path = output_dir.join("ymap_relationships.json");
  let Some(manifest) = manifest else {
    return Ok(false);
  };
  if !relationships_path.is_file() {
    return Ok(false);
  }
  if manifest.format_version != CACHE_SCHEMA
    || manifest.vanilla_manifest_sha256 != revision
    || manifest.latest_version != latest_version
    || manifest.vanilla_input_timestamps != *input_timestamps
  {
    return Ok(false);
  }
  let Ok(relationships) =
    crate::common::json::read_json::<YmapRelationshipIndex>(relationships_path)
  else {
    return Ok(false);
  };
  if relationships.format_version != CACHE_SCHEMA
    || relationships.vanilla_manifest_sha256 != revision
    || relationships.version != latest_version
  {
    return Ok(false);
  }
  if !output_dir.join("latest/ymap").is_dir() || !output_dir.join("latest/ybn").is_dir() {
    return Ok(false);
  }
  // Checked again once YMAP version diffs are generated; see `BuildVanillaCache::run_cache`.
  // if !ymap_diffs_are_current(output_dir, revision, latest_version) {
  //   return Ok(false);
  // }
  // Every link is read, tens of thousands in all, so they are checked in parallel. Links share
  // a few directories, so the path from each to the archive is worked out once.
  use rayon::prelude::*;
  let mut to_archive = BTreeMap::new();
  for file in manifest.files.values() {
    let link = output_dir.join(&file.object);
    let parent = link.parent().ok_or("Latest symlink has no parent")?.to_path_buf();
    to_archive.entry(parent).or_insert_with_key(|parent| relative_link_target(parent, vanilla_dir));
  }
  let link_is_current = |file: &LatestFile| -> Result<bool> {
    let link = output_dir.join(&file.object);
    if !fs::symlink_metadata(&link).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
      return Ok(false);
    }
    checked_source_path(vanilla_dir, &file.vanilla_object)?;
    let parent = link.parent().ok_or("Latest symlink has no parent")?;
    let expected_target = to_archive[parent].join(&file.vanilla_object);
    Ok(fs::read_link(link)? == expected_target)
  };
  let links = manifest.files.values().collect::<Vec<_>>();
  let stale = links
    .par_iter()
    .map(|file| link_is_current(file).map(|current| !current).map_err(|error| error.to_string()))
    .find_any(|result| !matches!(result, Ok(false)));
  match stale {
    None => Ok(true),
    Some(Ok(_)) => Ok(false),
    Some(Err(error)) => Err(error.into()),
  }
}
