//! Native snapshot reading and integrity-checked vanilla delta reconstruction.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde::Deserialize;

use super::{
  ModelReader, Result,
  comparison::ModelState,
  io::{cache_path, content_hash},
};
use crate::core::{
  codewalker::CodeWalker,
  format::ymap::model::Ymap,
  gtav_cache::{CachedFile, game_path, ymap_delta::VanillaYmapDelta},
};

#[derive(Deserialize)]
struct SavedDeltaFormat {
  #[serde(default)]
  format: Option<String>,
}

#[derive(Deserialize)]
struct RecoveredFile {
  stored: String,
  sha256: String,
}

pub(super) struct NativeVariants<'a> {
  pub(super) cache: &'a Path,
  pub(super) game_dir: &'a Path,
  pub(super) scratch: &'a Path,
  pub(super) reader: ModelReader,
  pub(super) codewalker: Option<CodeWalker>,
  pub(super) recovered: BTreeMap<String, PathBuf>,
  pub(super) models: BTreeMap<String, Rc<ModelState>>,
  pub(super) extractions: usize,
  pub(super) replaying: BTreeSet<String>,
}

impl NativeVariants<'_> {
  pub(super) fn model(
    &mut self,
    file: &CachedFile,
  ) -> Result<Rc<ModelState>> {
    if let Some(model) = self.models.get(&file.sha256) {
      return Ok(model.clone());
    }
    if file.sha256.len() != 64 || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
      return Err("Invalid native SHA-256 in GTAV cache".into());
    }
    if !self.replaying.insert(file.sha256.clone()) {
      return Err("Cyclic vanilla delta predecessor chain".into());
    }
    let result = self.load_model(file);
    self.replaying.remove(&file.sha256);
    let model = Rc::new(ModelState::new(result?)?);
    self.models.insert(file.sha256.clone(), model.clone());
    Ok(model)
  }

  /// Decode deltas directly: generic JSON objects can reorder entity keys, and
  /// untagged-enum buffering cannot preserve the generated numeric-key map decoder.
  fn load_model(
    &mut self,
    file: &CachedFile,
  ) -> Result<Ymap> {
    if file.object.ends_with(".diff.json") {
      let artifact = cache_path(self.cache, &file.object)?;
      if artifact.is_file() {
        let saved: SavedDeltaFormat =
          serde_json::from_reader(BufReader::new(fs::File::open(&artifact)?))?;
        if saved.format.is_some() {
          let delta: VanillaYmapDelta =
            serde_json::from_reader(BufReader::new(fs::File::open(artifact)?))?;
          if delta.target_native_sha256 != file.sha256 {
            return Err("Vanilla delta target native hash mismatch".into());
          }
          let before = self.model(&delta.base)?;
          log::info!("Reconstructing vanilla {} from original YMAP and JSON deltas", file.source);
          return delta.apply_to(&before.model);
        }
      }
    }
    let native = file
      .native
      .as_deref()
      .or_else(|| file.object.ends_with(".ymap").then_some(file.object.as_str()));
    let snapshot = native.map(|path| cache_path(self.cache, path)).transpose()?;
    let legacy = self.cache.join("objects").join(format!("{}.ymap", file.sha256));
    let path = if let Some(path) = snapshot.filter(|path| path.is_file()) {
      path
    } else if legacy.is_file() {
      legacy
    } else {
      if !self.recovered.contains_key(&file.sha256) {
        self.recover(file)?;
      }
      self.recovered.get(&file.sha256).cloned().ok_or_else(|| format!(
        "Installed RPF no longer matches cached content {} ({}). Regenerate the GTAV cache with native snapshots.",
        file.sha256, file.source,
      ))?
    };
    if content_hash(&path)? != file.sha256 {
      return Err(format!("Native snapshot hash mismatch: {}", path.display()).into());
    }
    (self.reader)(&path).map_err(|error| format!("Reading vanilla {}: {error}", file.source).into())
  }

  fn recover(
    &mut self,
    file: &CachedFile,
  ) -> Result<()> {
    if !self.game_dir.is_dir() {
      return Err(format!("Exact native snapshot is missing for {}; the original game directory {} is unavailable. Regenerate the GTAV cache with native snapshots.", file.source, self.game_dir.display()).into());
    }
    let parts: Vec<_> = file.source.split('/').collect();
    let outer =
      parts.iter().position(|part| part.ends_with(".rpf")).ok_or("Vanilla source has no RPF")?;
    if outer + 1 >= parts.len() {
      return Err("Vanilla source has no entry".into());
    }
    let relative = parts[..=outer].join("/");
    cache_path(self.game_dir, &relative)?;
    let archive = game_path(self.game_dir, Path::new(&relative))?;
    let subtree = parts[outer..parts.len() - 1].join("/");
    if self.codewalker.is_none() {
      let bridge =
        std::env::var_os("CODEWALKER_BRIDGE_DLL").map(PathBuf::from).unwrap_or_else(|| {
          PathBuf::from("bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll")
        });
      let codewalker = CodeWalker::init(&bridge)?;
      codewalker.load_game_keys(self.game_dir)?;
      self.codewalker = Some(codewalker);
    }
    log::info!("Recovering exact vanilla snapshots from {relative}: {subtree}");
    let directory = self.scratch.join(format!("rpf-{}", self.extractions));
    self.extractions += 1;
    self
      .codewalker
      .as_ref()
      .ok_or("CodeWalker is not initialized")?
      .extract_rpf_subtree(&archive, &subtree, &directory)?;
    let files: Vec<RecoveredFile> =
      serde_json::from_reader(BufReader::new(fs::File::open(directory.join("files.json"))?))?;
    for recovered in files {
      self.recovered.insert(recovered.sha256, cache_path(&directory, &recovered.stored)?);
    }
    Ok(())
  }
}
