//! Updates selected resource inventories and archives resources missing from a full scan.

use super::{
  CacheState, FileFingerprint, Publication, ResourceCheck, Result, SourceResources, VanillaInputs,
};
use crate::common::changes::StoreChanges;
use crate::common::json::write_json;
use crate::common::{filesystem::move_file, path::ensure_safe_path};
use crate::core::source_cache::types::ResourceInventory;
use crate::core::source_cache::{
  inventory::{fingerprint, resource_id, source_fingerprints, source_inventory},
  layout::{normalize_cached_layout, stream_relative_path},
};
use crate::platform::fs;
use rayon::prelude::*;
use std::{
  collections::BTreeMap,
  ops::Bound,
  path::Path,
  sync::atomic::{AtomicUsize, Ordering},
};

/// What updating one selected resource produced, merged into the cache state in resource order.
struct ResourceUpdate {
  id: String,
  /// The new inventory, source fingerprints and check of a resource whose cache changed.
  updated: Option<(ResourceInventory, BTreeMap<String, FileFingerprint>, ResourceCheck)>,
  changes: StoreChanges,
}

/// Updates every selected resource in parallel: each resource owns its cache directory and its
/// history, so resources share nothing but the read-only cache state.
pub(super) fn update_selected_resources(
  source: &SourceResources,
  vanilla: &VanillaInputs,
  publication: &Publication,
  state: &mut CacheState,
  force: bool,
) -> Result<bool> {
  let progress = Progress {
    completed: AtomicUsize::new(0),
    total: source.selected.len(),
  };
  let updates = source
    .selected
    .par_iter()
    .map(|resource| {
      update_resource(resource, source, vanilla, publication, state, force, &progress)
        .map_err(|error| error.to_string())
    })
    .collect::<std::result::Result<Vec<_>, String>>()?;
  let CacheState {
    inventories,
    checks,
    source_inputs,
    changes,
    ..
  } = state;
  let mut changed = false;
  for update in updates {
    changes.merge(update.changes);
    let Some((inventory, inputs, check)) = update.updated else {
      continue;
    };
    changed = true;
    let prefix = format!("{}/", update.id);
    source_inputs.retain(|key, _| !key.starts_with(&prefix));
    source_inputs.extend(inputs);
    inventories.insert(update.id.clone(), inventory);
    checks.insert(update.id, check);
  }
  Ok(changed)
}

/// Counts resources as they finish, for the progress log.
struct Progress {
  completed: AtomicUsize,
  total: usize,
}

impl Progress {
  /// The position of the resource that just finished, as `n/total`.
  fn next(&self) -> String {
    format!("{}/{}", self.completed.fetch_add(1, Ordering::Relaxed) + 1, self.total)
  }
}

/// The cached fingerprints of the inputs of the resource whose keys start with `prefix`.
fn inputs_under<'a>(
  inputs: &'a BTreeMap<String, FileFingerprint>,
  prefix: &'a str,
) -> impl Iterator<Item = (&'a String, &'a FileFingerprint)> {
  inputs
    .range::<str, _>((Bound::Included(prefix), Bound::Unbounded))
    .take_while(move |(key, _)| key.starts_with(prefix))
}

fn update_resource(
  resource: &Path,
  source: &SourceResources,
  vanilla: &VanillaInputs,
  publication: &Publication,
  state: &CacheState,
  force: bool,
  progress: &Progress,
) -> Result<ResourceUpdate> {
  let source_dir = &source.source_dir;
  let VanillaInputs {
    resource_revision,
    upstream_changed,
    derived,
    ..
  } = vanilla;
  let Publication {
    output,
    generated_at,
    ..
  } = publication;
  let CacheState {
    inventories,
    cached_inputs,
    checks,
    ..
  } = state;
  let mut changes = StoreChanges::default();
  let id = resource_id(source_dir, resource)?;
  let prefix = format!("{id}/");
  let cache_name = resource.file_name().ok_or("Resource has no name")?.to_string_lossy();
  let empty_inputs = BTreeMap::new();
  let current_inputs = source_fingerprints(
    source_dir,
    &[resource.to_path_buf()],
    if force { &empty_inputs } else { cached_inputs },
  )?;
  // Fingerprints taken anew: every one when forced, otherwise those added, changed or removed.
  let updated_fingerprints = if force {
    current_inputs.len()
  } else {
    let cached = inputs_under(cached_inputs, &prefix).collect::<BTreeMap<_, _>>();
    current_inputs.iter().filter(|(key, value)| cached.get(key) != Some(value)).count()
      + cached.keys().filter(|key| !current_inputs.contains_key(**key)).count()
  };
  let fingerprints_current = updated_fingerprints == 0;
  let previous_inventory = inventories.get(&id);
  let mut intact = previous_inventory.is_some();
  let mut layout_current = true;
  if let Some(inventory) = previous_inventory {
    for file in inventory.files_by_format.values().flatten() {
      let Some(path) = &file.cached_path else {
        continue;
      };
      let expected = format!("resources/{cache_name}/{}", stream_relative_path(&file.path)?);
      layout_current &= path == &expected;
      let cached_path = if path != &expected && !fs::is_file(output.join(path)) {
        output.join(&expected)
      } else {
        output.join(path)
      };
      ensure_safe_path(output, &cached_path)?;
      if !fs::is_file(&cached_path) {
        if fs::is_file(resource.join(&file.path)) {
          tracing::warn!(
            "Source-cache artifact is missing and will be restored from source: {}",
            cached_path.display()
          );
        } else {
          tracing::warn!(
            "Source file disappeared; dropping stale source-cache record: {}",
            cached_path.display()
          );
          changes.record(path, Some(&file.sha256), None);
        }
        intact = false;
        continue;
      }
      let recorded = FileFingerprint {
        sha256: file.sha256.clone(),
        size: file.size,
        modified_seconds: file.modified_seconds,
        modified_nanos: file.modified_nanos,
      };
      let current = fingerprint(&cached_path, if force { None } else { Some(&recorded) })?;
      intact &= current == recorded;
    }
  }
  let revision_current =
    checks.get(&id).is_some_and(|check| check.vanilla_revision == *resource_revision);
  if !*upstream_changed && revision_current && fingerprints_current && intact && layout_current {
    progress.next();
    return Ok(ResourceUpdate {
      id,
      updated: None,
      changes,
    });
  }
  let mut inventory = source_inventory(
    source_dir,
    &[resource.to_path_buf()],
    &current_inputs,
    &derived.files,
    if force { None } else { previous_inventory },
  )?
  .remove(&id)
  .ok_or("Resource inventory missing")?;
  let file_count: usize = inventory.files_by_format.values().map(Vec::len).sum();
  let cache_root = output.join("resources").join(cache_name.as_ref());
  let mut retained = previous_inventory.cloned();
  let mut targets = BTreeMap::new();
  for file in inventory
    .files_by_format
    .iter()
    .flat_map(|(format, files)| files.iter().filter(move |file| is_collected(format, file)))
    .chain(
      retained.iter().flat_map(|inventory| inventory.files_by_format.values().flatten()).filter(
        |file| file.cached_path.as_ref().is_some_and(|path| fs::is_file(output.join(path))),
      ),
    )
  {
    let relative = stream_relative_path(&file.path)?.to_ascii_lowercase();
    if let Some(previous) = targets.insert(relative.clone(), file.path.clone())
      && previous != file.path
    {
      return Err(
        format!(
          "Conflicting stream-relative cache paths in {id}: {previous} and {} -> {relative}",
          file.path
        )
        .into(),
      );
    }
  }
  if let Some(retained) = &mut retained {
    normalize_cached_layout(output, &cache_name, retained, &mut changes)?;
    for (format, files) in &retained.files_by_format {
      for file in files {
        let Some(cached_path) = &file.cached_path else {
          continue;
        };
        let mut file = file.clone();
        if !inventory.files_by_format.values().flatten().any(|incoming| incoming.path == file.path)
        {
          let recorded = FileFingerprint {
            sha256: file.sha256.clone(),
            size: file.size,
            modified_seconds: file.modified_seconds,
            modified_nanos: file.modified_nanos,
          };
          let path = output.join(cached_path);
          if !fs::is_file(&path) {
            continue;
          }
          let fingerprint = fingerprint(&path, if force { None } else { Some(&recorded) })?;
          if format == ".ymap" && (force || fingerprint.sha256 != file.sha256) {
            match crate::core::vanilla_cache::ymap_parent_hash(&path) {
              Ok(hash) => {
                file.ymap_parent_hash = Some(format!("{hash:08X}"));
                file.metadata_error = None;
              }
              Err(error) => {
                file.ymap_parent_hash = None;
                file.metadata_error = Some(error.to_string());
              }
            }
          }
          changes.record(cached_path, Some(&file.sha256), Some(&fingerprint.sha256));
          file.sha256 = fingerprint.sha256;
          file.size = fingerprint.size;
          file.modified_seconds = fingerprint.modified_seconds;
          file.modified_nanos = fingerprint.modified_nanos;
          inventory.files_by_format.entry(format.clone()).or_default().push(file);
        }
      }
    }
  }
  for (format, files) in &mut inventory.files_by_format {
    for file in files.iter_mut() {
      if file.cached_path.is_some() || !is_collected(format, file) {
        continue;
      }
      let relative = stream_relative_path(&file.path)?;
      let destination = cache_root.join(relative);
      ensure_safe_path(output, &destination)?;
      let mut replaced = None;
      // The version it replaces is kept by content, for overwrites written against it.
      if fs::exists(&destination).unwrap_or(false) {
        let old_fingerprint = fingerprint(&destination, None)?;
        crate::core::source_cache::history::archive(
          output,
          &cache_name,
          relative,
          &destination,
          &old_fingerprint.sha256,
        )?;
        replaced = Some(old_fingerprint.sha256);
      }
      fs::create_dir_all(destination.parent().ok_or("Cache file has no parent")?)?;
      move_file(&resource.join(&file.path), &destination)?;
      let cached_path = format!("resources/{cache_name}/{relative}");
      changes.record(&cached_path, replaced.as_deref(), Some(&file.sha256));
      file.cached_path = Some(cached_path);
    }
  }
  for files in inventory.files_by_format.values_mut() {
    files.sort_by(|left, right| left.path.cmp(&right.path));
  }
  // The stream files moved out, so the resource's remaining inputs are fingerprinted again.
  let inputs = source_fingerprints(source_dir, &[resource.to_path_buf()], &current_inputs)?;
  let check = ResourceCheck {
    checked_at: generated_at.clone(),
    vanilla_revision: resource_revision.clone(),
    has_stream: ["stream", "streams"].iter().any(|name| fs::is_dir(resource.join(name))),
  };
  tracing::info!(
    "Source cache updated {} for {id}: {updated_fingerprints} fingerprint, {file_count} stream files",
    progress.next()
  );
  Ok(ResourceUpdate {
    id,
    updated: Some((inventory, inputs, check)),
    changes,
  })
}

/// Whether the source cache moves `file`, of `format`, out of its resource: every file named
/// after a vanilla file, and every stream file a resource adds that the merge takes.
fn is_collected(
  format: &str,
  file: &crate::core::source_cache::types::SourceFile,
) -> bool {
  file.vanilla.is_some() || crate::core::source_cache::added::is_added_stream_file(format, file)
}

pub(super) fn remove_missing_resources(
  source: &SourceResources,
  publication: &Publication,
  state: &mut CacheState,
) -> Result<bool> {
  let SourceResources {
    selected_only,
    discovered,
    names,
    ..
  } = source;
  let Publication {
    output,
    history,
    previous,
    ..
  } = publication;
  let CacheState {
    inventories,
    source_inputs,
    checks,
    changes,
    ..
  } = state;
  let mut changed = false;
  if !selected_only {
    let removed =
      inventories.keys().filter(|id| !discovered.contains(*id)).cloned().collect::<Vec<_>>();
    for id in removed {
      changed = true;
      if let Some(inventory) = inventories.remove(&id) {
        for file in inventory.files_by_format.values().flatten() {
          if let Some(path) = &file.cached_path {
            changes.record(path, Some(&file.sha256), None);
          }
        }
        let name = inventory.source.file_name().ok_or("Resource has no name")?;
        if names.contains(&name.to_string_lossy().to_ascii_lowercase()) {
          return Err(
            format!(
              "Refusing to archive cache owned by a discovered resource: {}",
              name.to_string_lossy()
            )
            .into(),
          );
        }
        let current = output.join("resources").join(name);
        let cached =
          if fs::exists(&current).unwrap_or(false) { current } else { output.join(name) };
        if fs::exists(&cached).unwrap_or(false) {
          fs::create_dir_all(history.as_path())?;
          if let Some(previous) = previous {
            write_json(&history.join(crate::core::source_cache::CACHE_INFO), previous)?;
          }
          ensure_safe_path(output, &cached)?;
          fs::rename(cached, history.join(name))?;
        }
      }
      source_inputs.retain(|key, _| !key.starts_with(&format!("{id}/")));
      checks.remove(&id);
    }
  }
  Ok(changed)
}
