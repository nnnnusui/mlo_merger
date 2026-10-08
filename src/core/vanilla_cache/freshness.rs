use super::*;

pub(super) fn cache_is_current(
  output_dir: &Path,
  vanilla_dir: &Path,
  revision: &str,
  latest_version: &str,
  input_timestamps: &BTreeMap<String, SourceTimestamp>,
) -> Result<bool> {
  let manifest_path = output_dir.join("cache_info.json");
  let relationships_path = output_dir.join("ymap_relationships.json");
  if !manifest_path.is_file() || !relationships_path.is_file() {
    return Ok(false);
  }
  let Ok(manifest_file) = fs::File::open(manifest_path) else {
    return Ok(false);
  };
  let Ok(manifest) = serde_json::from_reader::<_, DerivedManifest>(BufReader::new(manifest_file))
  else {
    return Ok(false);
  };
  if manifest.format_version != CACHE_SCHEMA
    || manifest.vanilla_manifest_sha256 != revision
    || manifest.latest_version != latest_version
    || manifest.vanilla_input_timestamps != *input_timestamps
  {
    return Ok(false);
  }
  let Ok(relationships_file) = fs::File::open(relationships_path) else {
    return Ok(false);
  };
  let Ok(relationships) =
    serde_json::from_reader::<_, YmapRelationshipIndex>(BufReader::new(relationships_file))
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
  for file in manifest.files.values() {
    let link = output_dir.join(&file.object);
    if !fs::symlink_metadata(&link).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
      return Ok(false);
    }
    let source = checked_source_path(vanilla_dir, &file.vanilla_object)?;
    let expected_target =
      relative_link_target(link.parent().ok_or("Latest symlink has no parent")?, &source);
    if fs::read_link(link)? != expected_target {
      return Ok(false);
    }
  }
  Ok(true)
}
