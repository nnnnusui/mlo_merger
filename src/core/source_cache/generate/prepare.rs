//! Resolves inputs, reconciles cached resource identities and prepares publication paths.

use super::{
  BuildSourceCache, CacheState, Prepared, Publication, Result, SourceCacheMetadata,
  SourceResources, Staging, VanillaInputs,
};
use crate::common::{changes::StoreChanges, json::write_json, path::ensure_safe_path};
use crate::core::source_cache::{
  exclusions::{is_excluded, parse_exclusions},
  inventory::{discover_resources, resource_id, source_fingerprints},
  publication::{create_staging_directory, prepare_output_path},
};
use crate::core::{vanilla::load_manifest_with_revision, vanilla_cache::load_cache};
use crate::fs;
use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Path, PathBuf},
};

/// Rejects a source root holding none of the cached resources, which is a source tree other than
/// the one the cache was built from: treating every cached resource as removed would archive the
/// whole cache.
///
/// Resources are compared by directory name, as resources moving between groups are; where the
/// source root is does not matter.
fn check_same_source(
  metadata: &SourceCacheMetadata,
  resources: &[PathBuf],
  source_dir: &Path,
) -> Result<()> {
  if metadata.resources.is_empty() {
    return Ok(());
  }
  let names = resources
    .iter()
    .filter_map(|resource| resource.file_name())
    .map(|name| name.to_string_lossy().to_ascii_lowercase())
    .collect::<BTreeSet<_>>();
  let known = metadata.resources.values().any(|inventory| {
    inventory
      .source
      .file_name()
      .is_some_and(|name| names.contains(&name.to_string_lossy().to_ascii_lowercase()))
  });
  if known {
    return Ok(());
  }
  Err(
    format!(
      "None of the source cache's {} resources are in {}; is -i the source directory the cache was built from?",
      metadata.resources.len(),
      source_dir.display()
    )
    .into(),
  )
}

pub(super) fn prepare(build: &BuildSourceCache) -> Result<Prepared> {
  let selection = build.resource.as_deref();
  let source_dir = fs::canonicalize(&build.source_dir)?;
  let output = prepare_output_path(&build.output_dir)?;
  let exclusions = parse_exclusions(&build.exclusions.as_deref().unwrap_or_default().join(","))?;
  let all_resources = discover_resources(&source_dir)?;
  let resources = all_resources
    .iter()
    .cloned()
    .map(|resource| Ok((resource_id(&source_dir, &resource)?, resource)))
    .collect::<Result<Vec<_>>>()?
    .into_iter()
    .filter(|(id, _)| !is_excluded(id, &exclusions))
    .map(|(_, resource)| resource)
    .collect::<Vec<_>>();
  let mut selected = Vec::new();
  let mut names = BTreeSet::new();
  for resource in &resources {
    let name = resource.file_name().ok_or("Resource has no name")?.to_string_lossy();
    if name == "_old" || !names.insert(name.to_ascii_lowercase()) {
      return Err(format!("Duplicate or reserved resource name: {name}").into());
    }
    let id = resource_id(&source_dir, resource)?;
    if selection.is_none_or(|selection| selection == name || selection == id) {
      selected.push(resource.clone());
    }
  }
  if selection.is_some() && selected.is_empty() {
    return Err(
      format!("Resource not found or excluded: {}", selection.unwrap_or_default()).into(),
    );
  }
  let vanilla_dir = fs::canonicalize(&build.vanilla_dir)?;
  let vanilla_cache_dir = fs::canonicalize(&build.vanilla_cache_dir)?;
  let info = crate::core::source_cache::cache_info_path(&output);
  let previous = if fs::is_file(&info) {
    let metadata = crate::core::source_cache::read_metadata(&output)?;
    // Excluded resources still show that this is the cache's source tree.
    check_same_source(&metadata, &all_resources, &source_dir)?;
    Some(metadata)
  } else {
    None
  };
  let discovered = resources
    .iter()
    .map(|resource| resource_id(&source_dir, resource))
    .collect::<Result<BTreeSet<_>>>()?;
  let mut inventories =
    previous.as_ref().map(|metadata| metadata.resources.clone()).unwrap_or_default();
  let mut cached_inputs =
    previous.as_ref().map(|metadata| metadata.source_inputs.clone()).unwrap_or_default();
  let mut checks =
    previous.as_ref().map(|metadata| metadata.resource_checks.clone()).unwrap_or_default();
  let excluded_ids =
    inventories.keys().filter(|id| is_excluded(id, &exclusions)).cloned().collect::<Vec<_>>();
  let exclusions_changed = !excluded_ids.is_empty();
  let mut excluded_cache_paths = BTreeSet::new();
  // Excluded resources' cached files are archived before any resource is updated.
  let mut changes = StoreChanges::default();
  for id in excluded_ids {
    let inventory = inventories.get(&id).ok_or("Excluded resource inventory missing")?;
    for file in inventory.files_by_format.values().flatten() {
      if let Some(path) = &file.cached_path {
        changes.record(path, Some(&file.sha256), None);
      }
    }
    let name = inventory.source.file_name().ok_or("Resource has no name")?;
    for relative in [Path::new("resources").join(name), Path::new(name).to_path_buf()] {
      let cached = output.join(&relative);
      ensure_safe_path(&output, &cached)?;
      if fs::exists(&cached).unwrap_or(false) {
        for entry in crate::fs::WalkDir::new(&cached).follow_links(false) {
          let entry = entry?;
          ensure_safe_path(&output, entry.path())?;
        }
        excluded_cache_paths.insert(relative);
      }
    }
    inventories.remove(&id);
    cached_inputs.retain(|key, _| !key.starts_with(&format!("{id}/")));
    checks.remove(&id);
  }
  let mut cached_names = BTreeMap::new();
  let mut normalized_names = BTreeSet::new();
  for (id, inventory) in &inventories {
    let name = inventory.source.file_name().ok_or("Cached resource has no name")?;
    if !normalized_names.insert(name.to_string_lossy().to_ascii_lowercase()) {
      return Err(format!("Ambiguous cached resource name: {}", name.to_string_lossy()).into());
    }
    cached_names.insert(name.to_os_string(), id.clone());
  }
  let mut relocated = false;
  for resource in &selected {
    let id = resource_id(&source_dir, resource)?;
    let name = resource.file_name().ok_or("Resource has no name")?;
    let Some(old_id) = cached_names.get(name) else {
      continue;
    };
    if old_id == &id || discovered.contains(old_id) {
      continue;
    }
    let mut inventory = inventories.remove(old_id).ok_or("Cached resource missing")?;
    inventory.source = PathBuf::from(&id);
    inventories.insert(id.clone(), inventory);
    let prefix = format!("{old_id}/");
    let inputs =
      cached_inputs.keys().filter(|key| key.starts_with(&prefix)).cloned().collect::<Vec<_>>();
    for key in inputs {
      let fingerprint = cached_inputs.remove(&key).ok_or("Cached resource fingerprint missing")?;
      cached_inputs.insert(format!("{id}/{}", &key[prefix.len()..]), fingerprint);
    }
    if let Some(check) = checks.remove(old_id) {
      checks.insert(id.clone(), check);
    }
    relocated = true;
    tracing::info!("Relocated source resource {old_id} -> {id}");
  }
  let (_, manifest_sha256) = load_manifest_with_revision(&vanilla_dir)?;
  let derived = load_cache(&vanilla_cache_dir, &manifest_sha256)?;
  let derived_revision = derived.revision.clone();
  let resource_revision = format!("{manifest_sha256}:{derived_revision}");
  let upstream_changed = previous.as_ref().is_none_or(|metadata| {
    metadata.format_version != crate::core::source_cache::FORMAT_VERSION
      || metadata.vanilla_manifest_sha256 != manifest_sha256
      || metadata.vanilla_cache_revision != derived_revision
  });
  let empty_inputs = BTreeMap::new();
  let current_inputs = source_fingerprints(
    &source_dir,
    &selected,
    if build.force { &empty_inputs } else { &cached_inputs },
  )?;
  let source_inputs = cached_inputs.clone();
  let generated_at = chrono::Utc::now().to_rfc3339();
  let history =
    output.join("_old").join(chrono::Utc::now().format("%Y%m%dT%H%M%S%.9fZ").to_string());
  let changed = exclusions_changed
    || relocated
    || upstream_changed
    || previous
      .as_ref()
      .is_some_and(|metadata| metadata.outputs.iter().any(|name| !fs::is_file(output.join(name))));
  fs::create_dir_all(&output)?;
  ensure_safe_path(&output, &history)?;
  let staging = Staging(create_staging_directory(&output)?);
  Ok(Prepared {
    source: SourceResources {
      source_dir,
      resources,
      selected,
      names,
      discovered,
      selected_only: selection.is_some(),
    },
    vanilla: VanillaInputs {
      vanilla_dir,
      vanilla_cache_dir,
      manifest_sha256,
      derived,
      resource_revision,
      upstream_changed,
    },
    state: CacheState {
      inventories,
      cached_inputs,
      checks,
      current_inputs,
      source_inputs,
      changes,
    },
    publication: Publication {
      output,
      previous,
      generated_at,
      history,
      staging,
    },
    excluded_cache_paths,
    changed,
  })
}

pub(super) fn archive_excluded_resources(
  publication: &Publication,
  excluded_cache_paths: &BTreeSet<PathBuf>,
) -> Result<()> {
  let Publication {
    output,
    history,
    previous,
    ..
  } = publication;
  if !excluded_cache_paths.is_empty() {
    fs::create_dir_all(history.as_path())?;
    if let Some(previous) = previous {
      write_json(&history.join(crate::core::source_cache::CACHE_INFO), previous)?;
    }
    for relative in excluded_cache_paths.iter() {
      let archived = history.join(relative);
      fs::create_dir_all(archived.parent().ok_or("Excluded archive path has no parent")?)?;
      fs::rename(output.join(relative), archived)?;
    }
  }
  Ok(())
}
