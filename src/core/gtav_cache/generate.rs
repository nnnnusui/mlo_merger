//! Coordinates base archives, title updates and ordered DLC stages.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::{
  Result,
  archives::{
    DlcList, base_archives, dlc_archive, extract, game_path, patch_dlc, platform_archives,
  },
  io::{read_ymap, write_json},
  manifest::GtavCacheManifest,
  publication::{Staging, preserve_failed_logs, publish_cache},
  stage::stage,
};
use crate::core::codewalker::CodeWalker;

/// Builds a vanilla archive cache independently of the MLO merge pipeline.
#[derive(Debug, Clone)]
pub struct BuildGtavCache {
  /// Installed GTA V Legacy directory containing base RPFs and update/.
  pub game_dir: PathBuf,
  /// Cache directory containing cache_info.json and version directories.
  pub output_dir: PathBuf,
}

impl BuildGtavCache {
  /// Reads all installed root RPFs and ordered DLCs, saving additions, diffs, and version logs.
  /// Register `version_logger()` with the application logger to capture diff log records.
  pub fn run(
    &self,
    codewalker: &CodeWalker,
  ) -> Result<()> {
    let game_dir = self.game_dir.canonicalize()?;
    let roots = base_archives(&game_dir)?;
    game_path(&game_dir, Path::new("update/update.rpf"))?;
    codewalker.load_game_keys(&game_dir)?;
    fs::create_dir_all(&self.output_dir)?;
    let staging_path = self.output_dir.join(format!(".staging-{}", std::process::id()));
    fs::create_dir(&staging_path)?;
    let staging = Staging(staging_path);
    let build = staging.0.join("cache");
    fs::create_dir(&build)?;
    let result = self.build(codewalker, &game_dir, roots, &staging.0, &build);
    if result.is_err() {
      match preserve_failed_logs(&build, &self.output_dir) {
        Ok(path) => log::error!("Preserved failed cache logs at {}", path.display()),
        Err(error) => log::error!("Could not preserve failed cache logs: {error}"),
      }
    }
    result
  }

  fn build(
    &self,
    codewalker: &CodeWalker,
    game_dir: &Path,
    roots: Vec<String>,
    scratch: &Path,
    build: &Path,
  ) -> Result<()> {
    let mut manifest = GtavCacheManifest {
      format_version: 3,
      game_dir: game_dir.into(),
      versions: vec![],
    };
    let mut current = BTreeMap::new();
    let base_dir = scratch.join("base");
    stage(
      &mut manifest,
      &mut current,
      build,
      "base",
      || {
        let mut files = Vec::new();
        for relative in &roots {
          let directory = base_dir.join(relative);
          let extracted = extract(codewalker, game_dir, relative, &directory, None)?;
          files.extend(
            extracted
              .into_iter()
              .filter(|file| !file.source.contains("/dlcpacks/"))
              .map(|file| (file, directory.clone())),
          );
        }
        Ok((roots.clone(), files))
      },
      &read_ymap,
    )?;
    fs::remove_dir_all(&base_dir)?;

    let update_dir = scratch.join("update");
    let mut update = Vec::new();
    let mut dlc_items = Vec::new();
    stage(
      &mut manifest,
      &mut current,
      build,
      "update",
      || {
        update = extract(codewalker, game_dir, "update/update.rpf", &update_dir, None)?;
        let list = update
          .iter()
          .find(|file| file.source == "update/update.rpf/common/data/dlclist.xml")
          .ok_or("update/update.rpf/common/data/dlclist.xml is missing")?;
        let dlcs: DlcList = quick_xml::de::from_reader(std::io::BufReader::new(fs::File::open(
          update_dir.join(&list.stored),
        )?))?;
        dlc_items = dlcs.paths.items;
        Ok((
          vec!["update/update.rpf".into()],
          update
            .iter()
            .filter(|file| patch_dlc(&file.source).is_none())
            .cloned()
            .map(|file| (file, update_dir.clone()))
            .collect(),
        ))
      },
      &read_ymap,
    )?;

    let mut platform_index = None;
    for item in dlc_items {
      let relative = dlc_archive(&item)?.to_string_lossy().replace('\\', "/");
      let label = Path::new(&relative)
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .ok_or("DLC name is missing")?;
      let directory = scratch.join("dlc");
      stage(
        &mut manifest,
        &mut current,
        build,
        label,
        || {
          let (physical, subtree) =
            if relative.starts_with("x64/") && game_path(game_dir, Path::new(&relative)).is_err() {
              if platform_index.is_none() {
                platform_index = Some(platform_archives(codewalker, game_dir, scratch)?);
              }
              let (physical, subtree) = platform_index
                .as_ref()
                .unwrap()
                .get(&relative)
                .ok_or_else(|| format!("Listed platform DLC archive not found: {item}"))?;
              (physical.clone(), Some(subtree.clone()))
            } else {
              (relative.clone(), None)
            };
          let extracted = extract(codewalker, game_dir, &physical, &directory, subtree.as_deref())?;
          let mut files: Vec<_> =
            extracted.into_iter().map(|file| (file, directory.clone())).collect();
          let patches: Vec<_> =
            update.iter().filter(|file| patch_dlc(&file.source) == Some(label)).cloned().collect();
          let mut archives = vec![subtree.unwrap_or(physical)];
          if !patches.is_empty() {
            archives.push(format!("update/update.rpf/dlc_patch/{label}"));
            files.extend(patches.into_iter().map(|file| (file, update_dir.clone())));
          }
          Ok((archives, files))
        },
        &read_ymap,
      )?;
      fs::remove_dir_all(directory)?;
    }
    write_json(&build.join("cache_info.json"), &manifest)?;
    publish_cache(build, &self.output_dir, &manifest)?;
    log::info!("Saved {} vanilla stages to {}", manifest.versions.len(), self.output_dir.display());
    Ok(())
  }
}
