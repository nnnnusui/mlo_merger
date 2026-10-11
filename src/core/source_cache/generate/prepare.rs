//! Resolves inputs, reconciles cached resource identities and prepares publication paths.

use super::{
  BuildSourceCache, CacheState, Prepared, Publication, Result, SourceCacheMetadata,
  SourceResources, Staging, VanillaInputs,
};
use crate::core::source_cache::{
  exclusions::{FILE_NAME as EXCLUSIONS_FILE, is_excluded, parse_exclusions},
  filesystem::ensure_safe_path,
  inventory::{discover_resources, resource_id, source_fingerprints},
  publication::{create_staging_directory, prepare_output_path},
};
use crate::core::{
  vanilla::{load_manifest_with_revision, write_json},
  vanilla_cache::{BuildVanillaCache, load_cache},
};
use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  io::BufReader,
  path::{Path, PathBuf},
};

pub(super) fn prepare(build: &BuildSourceCache) -> Result<Prepared> {
  let selection = build.resource.as_deref();
  let source_dir = build.source_dir.canonicalize()?;
  let output = prepare_output_path(&build.output_dir)?;
  let exclusions_path = output.join(EXCLUSIONS_FILE);
  ensure_safe_path(&output, &exclusions_path)?;
  let saved_exclusions: Vec<String> = if exclusions_path.is_file() {
    serde_json::from_reader(BufReader::new(fs::File::open(&exclusions_path)?))?
  } else {
    Vec::new()
  };
  let exclusions = match build.exclusions.as_deref() {
    Some(patterns) => parse_exclusions(&patterns.join(","))?,
    None => parse_exclusions(&saved_exclusions.join(","))?,
  };
  let resources = discover_resources(&source_dir)?
    .into_iter()
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
  let vanilla_dir = build.vanilla_dir.canonicalize()?;
  BuildVanillaCache {
    vanilla_dir: vanilla_dir.clone(),
    output_dir: build.vanilla_cache_dir.clone(),
    through_version: None,
    force: false,
  }
  .run()?;
  let vanilla_cache_dir = build.vanilla_cache_dir.canonicalize()?;
  let previous = if output.join("source_cache_info.json").is_file() {
    let metadata: SourceCacheMetadata = serde_json::from_reader(BufReader::new(fs::File::open(
      output.join("source_cache_info.json"),
    )?))?;
    if metadata.source_dir != source_dir || !matches!(metadata.format_version, 2 | 3) {
      return Err("Source cache belongs to another input directory or unsupported schema".into());
    }
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
  for id in excluded_ids {
    let inventory = inventories.get(&id).ok_or("Excluded resource inventory missing")?;
    let name = inventory.source.file_name().ok_or("Resource has no name")?;
    for relative in [Path::new("resources").join(name), Path::new(name).to_path_buf()] {
      let cached = output.join(&relative);
      ensure_safe_path(&output, &cached)?;
      if cached.exists() {
        for entry in walkdir::WalkDir::new(&cached).follow_links(false) {
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
    inventory.source = resource.clone();
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
    log::info!("Relocated source resource {old_id} -> {id}");
  }
  let (_, manifest_sha256) = load_manifest_with_revision(&vanilla_dir)?;
  let derived = load_cache(&vanilla_cache_dir, &manifest_sha256)?;
  let derived_revision = derived.revision.clone();
  let resource_revision = format!("{manifest_sha256}:{derived_revision}");
  let upstream_changed = previous.as_ref().is_none_or(|metadata| {
    metadata.format_version != 3
      || metadata.vanilla_dir != vanilla_dir
      || metadata.vanilla_cache_dir != vanilla_cache_dir
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
  let changed = saved_exclusions != exclusions
    || exclusions_changed
    || relocated
    || upstream_changed
    || previous
      .as_ref()
      .is_some_and(|metadata| metadata.outputs.iter().any(|name| !output.join(name).is_file()));
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
      exclusions,
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
      write_json(&history.join("source_cache_info.json"), previous)?;
    }
    for relative in excluded_cache_paths.iter() {
      let archived = history.join(relative);
      fs::create_dir_all(archived.parent().ok_or("Excluded archive path has no parent")?)?;
      fs::rename(output.join(relative), archived)?;
    }
  }
  Ok(())
}
