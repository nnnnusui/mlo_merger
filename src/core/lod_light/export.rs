use super::{
  io::{Fingerprint, atomic_json, reject_symlinks, safe_relative},
  resource::{ResourceFile, inventory_for},
  snapshot::{closure, document, read_map},
  *,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io::BufReader, path::PathBuf};

const EXPORT_VERSION: u32 = 3;

/// Cached, resource-local export without running or mutating the asset pipeline.
#[derive(Debug, Clone)]
pub struct ExportLights {
  /// Native YMAP path from the live resource or active source cache.
  pub input: PathBuf,
  /// Unique resource name or exact source-relative identifier, including bracket groups.
  pub resource: String,
  /// Root containing the FiveM source resources.
  pub source_dir: PathBuf,
  /// Active source cache used to resolve moved inputs.
  pub source_cache_dir: PathBuf,
  /// JSON root; resource and stream-relative paths are retained underneath.
  pub output_dir: PathBuf,
  /// Bypass stat-based fingerprint and output reuse.
  pub force: bool,
}

/// Result of an on-demand export.
#[derive(Debug, Clone)]
pub struct ExportLightResult {
  /// JSON path suitable for reading by the external resource.
  pub path: PathBuf,
  /// True when cached native parsing and JSON serialization were skipped.
  pub skipped: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[cfg_attr(feature = "typescript", specta(rename = "LightExportCache"))]
struct ExportCache {
  version: u32,
  inputs: BTreeMap<String, CachedInput>,
  selected: std::collections::BTreeSet<String>,
  output: Fingerprint,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[cfg_attr(feature = "typescript", specta(rename = "LightExportInput"))]
struct CachedInput {
  path: PathBuf,
  fingerprint: Fingerprint,
  metadata: super::metadata::LightMetadata,
}

struct Lock(PathBuf);
impl Drop for Lock {
  fn drop(&mut self) {
    let _ = fs::remove_file(&self.0);
  }
}

impl ExportLights {
  /// Exports linked light views, reusing unchanged JSON and dependency fingerprints.
  ///
  /// ```no_run
  /// # use mlo_merger::core::lod_light::ExportLights;
  /// let result = ExportLights {
  ///   input: "asset/source/maps/stream/map_lodlights.ymap".into(),
  ///   resource: "maps".into(), source_dir: "asset/source".into(),
  ///   source_cache_dir: "asset/source-cache".into(),
  ///   output_dir: "asset/read_cache/ymap".into(), force: false,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<ExportLightResult> {
    safe_relative(&self.resource)?;
    let inventory = inventory_for(&self.source_dir, &self.source_cache_dir, &self.resource)?;
    let (resource, files) = inventory.iter().next().ok_or("Unknown or excluded light resource")?;
    reject_symlinks(&self.input)?;
    let input = self.input.canonicalize()?;
    let mut targets = files.iter().filter_map(|(relative, file)| {
      (file.path.canonicalize().ok().as_ref() == Some(&input)).then_some(relative)
    });
    let target = targets.next().ok_or("Input does not belong to selected resource")?;
    if targets.next().is_some() {
      return Err("Ambiguous light input".into());
    }
    let output = self.output_dir.join(resource).join(format!("{target}.json"));
    reject_symlinks(&output)?;
    if files.values().any(|file| file.path == output) {
      return Err("JSON would overwrite source input".into());
    }
    fs::create_dir_all(output.parent().ok_or("Missing output directory")?)?;
    let lock_path = output.with_extension("json.lock");
    fs::OpenOptions::new().write(true).create_new(true).open(&lock_path).map_err(|error| {
      format!("Export is locked (retry, or remove a stale lock): {}: {error}", lock_path.display())
    })?;
    let _lock = Lock(lock_path);
    let cache_path = output.with_extension("json.cache_info.json");
    reject_symlinks(&cache_path)?;
    let previous = fs::File::open(&cache_path)
      .ok()
      .and_then(|file| serde_json::from_reader::<_, ExportCache>(BufReader::new(file)).ok());
    let mut inputs = BTreeMap::new();
    for (relative, file) in files {
      let old = previous.as_ref().and_then(|cache| cache.inputs.get(relative));
      let fingerprint = Fingerprint::read(
        &file.path,
        old.filter(|old| old.path == file.path).map(|old| &old.fingerprint),
        self.force,
      )?;
      let metadata = if !self.force
        && let Some(old) = old
        && old.fingerprint.sha256 == fingerprint.sha256
      {
        old.metadata.clone()
      } else {
        super::metadata::read(file)?
      };
      inputs.insert(
        relative.clone(),
        CachedInput {
          path: file.path.clone(),
          fingerprint,
          metadata,
        },
      );
    }
    let metadata =
      inputs.iter().map(|(path, input)| (path.clone(), input.metadata.clone())).collect();
    let selected = closure(target, files, &metadata)?;
    let unchanged = !self.force
      && previous.as_ref().is_some_and(|old| {
        old.version == EXPORT_VERSION
          && old.selected == selected
          && selected.iter().all(|path| {
            let input = &inputs[path];
            old.inputs.get(path).is_some_and(|old| {
              old.fingerprint.sha256 == input.fingerprint.sha256 && old.metadata == input.metadata
            })
          })
      });
    let previous_output = previous.as_ref().map(|cache| &cache.output);
    let current_output = if output.is_file() {
      Some(Fingerprint::read(&output, previous_output, self.force)?)
    } else {
      None
    };
    if unchanged
      && current_output
        .as_ref()
        .zip(previous_output)
        .is_some_and(|(current, old)| current.sha256 == old.sha256)
    {
      let cache = ExportCache {
        version: EXPORT_VERSION,
        inputs,
        selected,
        output: current_output.ok_or("Missing JSON fingerprint")?,
      };
      if previous.as_ref() != Some(&cache) {
        atomic_json(&cache_path, &cache)?;
      }
      return Ok(ExportLightResult {
        path: output,
        skipped: true,
      });
    }
    let fingerprints =
      inputs.iter().map(|(path, input)| (path.clone(), input.fingerprint.clone())).collect();
    let maps = selected
      .iter()
      .map(|path| Ok((path.clone(), read_map(&files[path])?)))
      .collect::<Result<BTreeMap<_, _>>>()?;
    let document = document(resource, target, &maps, &fingerprints)?;
    verify_inputs(files, &inputs)?;
    atomic_json(&output, &document)?;
    atomic_json(
      &cache_path,
      &ExportCache {
        version: EXPORT_VERSION,
        inputs,
        selected,
        output: Fingerprint::read(&output, None, true)?,
      },
    )?;
    Ok(ExportLightResult {
      path: output,
      skipped: false,
    })
  }
}

fn verify_inputs(
  files: &BTreeMap<String, ResourceFile>,
  inputs: &BTreeMap<String, CachedInput>,
) -> Result<()> {
  for (path, input) in inputs {
    if Fingerprint::read(&files[path].path, Some(&input.fingerprint), false)? != input.fingerprint {
      return Err(format!("Light input changed while exporting: {path}").into());
    }
  }
  Ok(())
}

pub(crate) fn uncached_snapshot(
  resource: &str,
  target: &str,
  files: &BTreeMap<String, ResourceFile>,
) -> Result<(YmapLightDocument, BTreeMap<String, LoadedMap>)> {
  let metadata = files
    .iter()
    .map(|(path, file)| Ok((path.clone(), super::metadata::read(file)?)))
    .collect::<Result<BTreeMap<_, _>>>()?;
  let selected = closure(target, files, &metadata)?;
  let mut fingerprints = BTreeMap::new();
  let mut maps = BTreeMap::new();
  for path in selected {
    let before = Fingerprint::read(&files[&path].path, None, true)?;
    let map = read_map(&files[&path])?;
    if before != Fingerprint::read(&files[&path].path, Some(&before), false)? {
      return Err("Light input changed while reading snapshot".into());
    }
    fingerprints.insert(path.clone(), before);
    maps.insert(path, map);
  }
  Ok((document(resource, target, &maps, &fingerprints)?, maps))
}
