//! Materializes one ordered vanilla cache stage.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::{
  Result,
  archives::ExtractedFile,
  io::write_json,
  logging::VersionLog,
  manifest::{CacheVersion, CachedFile, FileChange, GtavCacheManifest},
  ymap_delta,
};
use crate::core::{format::ymap::model::Ymap, merge::YmapDiff};

pub(super) fn stage(
  manifest: &mut GtavCacheManifest,
  current: &mut BTreeMap<String, CachedFile>,
  output: &Path,
  label: &str,
  files: impl FnOnce() -> Result<(Vec<String>, Vec<(ExtractedFile, PathBuf)>)>,
  reader: &impl Fn(&Path) -> Result<Ymap>,
) -> Result<()> {
  let id = format!("{:04}-{label}", manifest.versions.len());
  let directory = output.join(&id);
  fs::create_dir(&directory)?;
  fs::create_dir(directory.join("ymap"))?;
  fs::create_dir_all(output.join(".working"))?;
  let version_log = VersionLog::start(&directory)?;
  let result = (|| -> Result<()> {
    log::info!("Creating cache {id}");
    let (archives, files) = files()?;
    log::info!("Processing archives: {}", archives.join(", "));
    let mut winners = BTreeMap::new();
    for (file, directory) in files {
      if file.name.ends_with(".ymap") {
        if file.name.contains(['/', '\\']) || file.name == ".ymap" {
          return Err(format!("Invalid YMAP basename: {}", file.name).into());
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
      let working = output.join(".working").join(&name);
      let object = if previous.is_some() {
        format!("{id}/ymap/{name}.diff.json")
      } else {
        format!("{id}/ymap/{name}")
      };
      let destination = output.join(&object);
      if let Some(previous) = previous {
        log::info!("Diff {name}: {} -> {}", previous.source, file.source);
        let before =
          reader(&working).map_err(|error| format!("Reading previous {name}: {error}"))?;
        let after =
          reader(&input).map_err(|error| format!("Reading replacement {name}: {error}"))?;
        let diff = YmapDiff::extract_from(&before, &after);
        drop(diff);
        let delta = ymap_delta::VanillaYmapDelta::extract_from(
          &before,
          &after,
          previous.clone(),
          file.sha256.clone(),
        )?;
        write_json(&destination, &delta)?;
      } else {
        log::info!("Added {name}: {}", file.source);
        fs::copy(&input, &destination)?;
      }
      fs::copy(&input, &working)?;
      let cached = CachedFile {
        sha256: file.sha256,
        object,
        native: None,
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
      "{id}: {} additions/replacements, {unchanged} unchanged, {} effective YMAPs",
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
