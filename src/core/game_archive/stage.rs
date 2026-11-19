//! Materializes one ordered game-archive stage.

use crate::platform::fs;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{
  Result,
  archives::ExtractedFile,
  manifest::{CacheVersion, CachedFile, FileChange, VanillaCacheManifest},
};
use crate::common::json::write_json;

pub(crate) fn stage(
  manifest: &mut VanillaCacheManifest,
  current: &mut BTreeMap<String, CachedFile>,
  output: &Path,
  label: &str,
  extensions: &[&str],
  files: impl FnOnce() -> Result<(Vec<String>, Vec<(ExtractedFile, PathBuf)>)>,
) -> Result<()> {
  let id = format!("{:04}-{label}", manifest.versions.len());
  let directory = output.join(&id);
  let span = tracing::info_span!("vanilla_cache_stage", stage = %id);
  let _entered = span.enter();
  let result = (|| -> Result<()> {
    tracing::info!("Creating cache {id}");
    let (archives, files) = files()?;
    tracing::info!("Processing archives: {}", archives.join(", "));
    let mut winners = BTreeMap::new();
    for (file, directory) in files {
      if let Some((stem, extension)) = file.name.rsplit_once('.')
        && extensions.contains(&extension)
      {
        if file.name.contains(['/', '\\']) || stem.is_empty() {
          return Err(format!("Invalid native basename: {}", file.name).into());
        }
        if let Some((previous, _)) = winners.get(&file.name) {
          let previous: &ExtractedFile = previous;
          if previous.sha256 != file.sha256 {
            tracing::warn!(
              "Within-stage override {}: {} -> {}",
              file.name,
              previous.source,
              file.source
            );
          }
        }
        winners.insert(file.name.clone(), (file, directory));
      }
    }
    let mut changes = BTreeMap::new();
    let mut file_kinds = BTreeMap::new();
    let mut unchanged = 0;
    for (name, (file, directory)) in winners {
      let previous = current.get(&name);
      if previous.is_some_and(|previous| previous.sha256 == file.sha256) {
        unchanged += 1;
        tracing::debug!("Unchanged {name}: {}", file.source);
        continue;
      }
      let input = directory.join(&file.stored);
      let family = Path::new(&name)
        .extension()
        .and_then(|extension| extension.to_str())
        .ok_or_else(|| format!("Invalid native basename: {name}"))?;
      let object = format!("{id}/{family}/{name}");
      let destination = output.join(&object);
      if let Some(previous) = previous {
        tracing::info!("Modified {name}: {} -> {}", previous.source, file.source);
      } else {
        tracing::info!("Added {name}: {}", file.source);
      }
      fs::create_dir_all(output.join(&id).join(family))?;
      fs::copy(&input, &destination)?;
      let cached = CachedFile {
        sha256: file.sha256,
        object,
        source: file.source,
      };
      if let Some(kind) = file.kind {
        file_kinds.insert(name.clone(), kind.to_string());
      }
      changes.insert(
        name.clone(),
        FileChange {
          previous_sha256: previous.map(|previous| previous.sha256.clone()),
          file: cached.clone(),
        },
      );
      current.insert(name, cached);
    }
    tracing::info!(
      "{id}: {} additions/replacements, {unchanged} unchanged, {} effective native files",
      changes.len(),
      current.len()
    );
    let version = CacheVersion {
      id,
      parent: manifest.versions.last().map(|version| version.id.clone()),
      archives,
      changes,
      file_kinds,
      unchanged,
      dlc: None,
    };
    if !version.changes.is_empty() {
      write_json(&directory.join("version_info.json"), &version)?;
    }
    manifest.versions.push(version);
    Ok(())
  })();
  if let Err(error) = &result {
    tracing::error!("Cache creation failed: {error}");
  }
  result
}
