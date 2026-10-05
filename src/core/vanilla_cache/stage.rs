//! Materializes one ordered vanilla cache stage.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::{
  Result,
  archives::ExtractedFile,
  io::write_json,
  logging::VersionLog,
  manifest::{CacheVersion, CachedFile, FileChange, VanillaCacheManifest},
};

pub(super) fn stage(
  manifest: &mut VanillaCacheManifest,
  current: &mut BTreeMap<String, CachedFile>,
  output: &Path,
  label: &str,
  files: impl FnOnce() -> Result<(Vec<String>, Vec<(ExtractedFile, PathBuf)>)>,
) -> Result<()> {
  let id = format!("{:04}-{label}", manifest.versions.len());
  let directory = output.join(&id);
  fs::create_dir(&directory)?;
  fs::create_dir(directory.join("ymap"))?;
  fs::create_dir(directory.join("ybn"))?;
  let version_log = VersionLog::start(&directory)?;
  let result = (|| -> Result<()> {
    log::info!("Creating cache {id}");
    let (archives, files) = files()?;
    log::info!("Processing archives: {}", archives.join(", "));
    let mut winners = BTreeMap::new();
    for (file, directory) in files {
      if file.name.ends_with(".ymap") || file.name.ends_with(".ybn") {
        if file.name.contains(['/', '\\']) || matches!(file.name.as_str(), ".ymap" | ".ybn") {
          return Err(format!("Invalid native basename: {}", file.name).into());
        }
        if let Some((previous, _)) = winners.get(&file.name) {
          let previous: &ExtractedFile = previous;
          if previous.sha256 != file.sha256 {
            log::warn!(
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
    let mut unchanged = 0;
    for (name, (file, directory)) in winners {
      let previous = current.get(&name);
      if previous.is_some_and(|previous| previous.sha256 == file.sha256) {
        unchanged += 1;
        log::debug!("Unchanged {name}: {}", file.source);
        continue;
      }
      let input = directory.join(&file.stored);
      let family = if name.ends_with(".ybn") { "ybn" } else { "ymap" };
      let object = format!("{id}/{family}/{name}");
      let destination = output.join(&object);
      if let Some(previous) = previous {
        log::info!("Modified {name}: {} -> {}", previous.source, file.source);
      } else {
        log::info!("Added {name}: {}", file.source);
      }
      fs::copy(&input, &destination)?;
      let cached = CachedFile {
        sha256: file.sha256,
        object,
        source: file.source,
      };
      changes.insert(
        name.clone(),
        FileChange {
          previous_sha256: previous.map(|previous| previous.sha256.clone()),
          file: cached.clone(),
        },
      );
      current.insert(name, cached);
    }
    log::info!(
      "{id}: {} additions/replacements, {unchanged} unchanged, {} effective native files",
      changes.len(),
      current.len()
    );
    let version = CacheVersion {
      id,
      parent: manifest.versions.last().map(|version| version.id.clone()),
      archives,
      changes,
      unchanged,
    };
    write_json(&directory.join("version_info.json"), &version)?;
    manifest.versions.push(version);
    Ok(())
  })();
  if let Err(error) = &result {
    log::error!("Cache creation failed: {error}");
  }
  version_log.finish()?;
  result
}
