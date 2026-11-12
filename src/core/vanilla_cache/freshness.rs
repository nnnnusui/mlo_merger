use super::*;
use std::sync::Arc;

/// Reads the manifest the cache was last published with, if it is readable.
pub(super) fn previous_manifest(output_dir: &Path) -> Option<Arc<read::LoadedDerived>> {
  read::read_derived(output_dir).ok()
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
  if !fs::is_file(&relationships_path) {
    return Ok(false);
  }
  if manifest.format_version != CACHE_SCHEMA
    || manifest.vanilla_dir != recorded_vanilla_dir(output_dir, vanilla_dir)
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
  if !generations::index_is_current(output_dir, revision, latest_version) {
    return Ok(false);
  }
  // Checked again once YMAP version diffs are generated; see `BuildVanillaCache::run_cache`.
  // if !ymap_diffs_are_current(output_dir, revision, latest_version) {
  //   return Ok(false);
  // }
  Ok(true)
}
