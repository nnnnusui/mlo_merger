//! Updates selected resource inventories and archives resources missing from a full scan.

use super::{
  CacheState, FileFingerprint, Publication, ResourceCheck, Result, SourceResources, VanillaInputs,
};
use crate::common::json::write_json;
use crate::common::{filesystem::move_file, path::ensure_safe_path};
use crate::core::source_cache::{
  inventory::{fingerprint, resource_id, source_fingerprints, source_inventory},
  layout::{normalize_cached_layout, stream_relative_path},
};
use std::{collections::BTreeMap, fs};

pub(super) fn update_selected_resources(
  source: &SourceResources,
  vanilla: &VanillaInputs,
  publication: &Publication,
  state: &mut CacheState,
  force: bool,
) -> Result<bool> {
  let SourceResources {
    source_dir,
    selected,
    ..
  } = source;
  let VanillaInputs {
    resource_revision,
    upstream_changed,
    derived,
    ..
  } = vanilla;
  let Publication {
    output,
    history,
    previous,
    generated_at,
    ..
  } = publication;
  let CacheState {
    inventories,
    cached_inputs,
    checks,
    current_inputs,
    source_inputs,
  } = state;
  let mut changed = false;
  for resource in selected.iter() {
    let id = resource_id(source_dir, resource)?;
    let prefix = format!("{id}/");
    let cache_name = resource.file_name().ok_or("Resource has no name")?.to_string_lossy();
    let current =
      current_inputs.iter().filter(|(key, _)| key.starts_with(&prefix)).collect::<BTreeMap<_, _>>();
    let cached =
      cached_inputs.iter().filter(|(key, _)| key.starts_with(&prefix)).collect::<BTreeMap<_, _>>();
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
        let cached_path = if path != &expected && !output.join(path).is_file() {
          output.join(&expected)
        } else {
          output.join(path)
        };
        ensure_safe_path(output, &cached_path)?;
        if !cached_path.is_file() {
          if resource.join(&file.path).is_file() {
            tracing::warn!(
              "Source-cache artifact is missing and will be restored from source: {}",
              cached_path.display()
            );
          } else {
            tracing::warn!(
              "Source file disappeared; dropping stale source-cache record: {}",
              cached_path.display()
            );
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
    if !force
      && !*upstream_changed
      && revision_current
      && current == cached
      && intact
      && layout_current
    {
      continue;
    }
    changed = true;
    tracing::info!("Updating source resource {id}");
    let mut inventory = source_inventory(
      source_dir,
      std::slice::from_ref(resource),
      current_inputs,
      &derived.files,
      if force { None } else { previous_inventory },
    )?
    .remove(&id)
    .ok_or("Resource inventory missing")?;
    let cache_root = output.join("resources").join(cache_name.as_ref());
    let mut retained = previous_inventory.cloned();
    let mut targets = BTreeMap::new();
    for file in
      inventory.files_by_format.values().flatten().filter(|file| file.vanilla.is_some()).chain(
        retained
          .iter()
          .flat_map(|inventory| inventory.files_by_format.values().flatten())
          .filter(|file| file.cached_path.as_ref().is_some_and(|path| output.join(path).is_file())),
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
      normalize_cached_layout(output, &cache_name, retained)?;
      for (format, files) in &retained.files_by_format {
        for file in files {
          let Some(cached_path) = &file.cached_path else {
            continue;
          };
          let mut file = file.clone();
          if !inventory
            .files_by_format
            .values()
            .flatten()
            .any(|incoming| incoming.path == file.path)
          {
            let recorded = FileFingerprint {
              sha256: file.sha256.clone(),
              size: file.size,
              modified_seconds: file.modified_seconds,
              modified_nanos: file.modified_nanos,
            };
            let path = output.join(cached_path);
            if !path.is_file() {
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
            file.sha256 = fingerprint.sha256;
            file.size = fingerprint.size;
            file.modified_seconds = fingerprint.modified_seconds;
            file.modified_nanos = fingerprint.modified_nanos;
            inventory.files_by_format.entry(format.clone()).or_default().push(file);
          }
        }
      }
    }
    for file in inventory.files_by_format.values_mut().flatten() {
      if file.cached_path.is_some() || file.vanilla.is_none() {
        continue;
      }
      let relative = stream_relative_path(&file.path)?;
      let destination = cache_root.join(relative);
      ensure_safe_path(output, &destination)?;
      if destination.exists() {
        fs::create_dir_all(history.as_path())?;
        if !history.join("source_cache_info.json").exists()
          && let Some(previous) = previous
        {
          write_json(&history.join("source_cache_info.json"), previous)?;
        }
        let archived = history.join(cache_name.as_ref()).join(relative);
        fs::create_dir_all(archived.parent().ok_or("Archive path has no parent")?)?;
        let old_fingerprint = fingerprint(&destination, None)?;
        write_json(
          &archived.with_file_name(format!(
            "{}.fingerprint.json",
            archived.file_name().unwrap().to_string_lossy()
          )),
          &old_fingerprint,
        )?;
        move_file(&destination, &archived)?;
      }
      fs::create_dir_all(destination.parent().ok_or("Cache file has no parent")?)?;
      move_file(&resource.join(&file.path), &destination)?;
      file.cached_path = Some(format!("resources/{cache_name}/{relative}"));
    }
    for files in inventory.files_by_format.values_mut() {
      files.sort_by(|left, right| left.path.cmp(&right.path));
    }
    inventories.insert(id.clone(), inventory);
    source_inputs.retain(|key, _| !key.starts_with(&prefix));
    source_inputs.extend(source_fingerprints(
      source_dir,
      std::slice::from_ref(resource),
      current_inputs,
    )?);
    checks.insert(
      id,
      ResourceCheck {
        checked_at: generated_at.clone(),
        vanilla_revision: resource_revision.clone(),
        has_stream: ["stream", "streams"].iter().any(|name| resource.join(name).is_dir()),
      },
    );
  }
  Ok(changed)
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
    ..
  } = state;
  let mut changed = false;
  if !selected_only {
    let removed =
      inventories.keys().filter(|id| !discovered.contains(*id)).cloned().collect::<Vec<_>>();
    for id in removed {
      changed = true;
      if let Some(inventory) = inventories.remove(&id) {
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
        let cached = if current.exists() { current } else { output.join(name) };
        if cached.exists() {
          fs::create_dir_all(history.as_path())?;
          if let Some(previous) = previous {
            write_json(&history.join("source_cache_info.json"), previous)?;
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
