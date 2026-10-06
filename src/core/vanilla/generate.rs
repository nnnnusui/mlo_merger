//! Coordinates base archives, title updates and ordered DLC stages.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::{
  Result,
  archives::{
    DlcList, base_archives, dlc_archive, extract, game_path, patch_dlc, platform_archives,
  },
  hash_index::write_hash_index,
  io::write_json,
  manifest::VanillaCacheManifest,
  publication::{Staging, preserve_failed_logs, publish_cache, reset_output_directory},
  stage::stage,
};
use crate::core::codewalker::CodeWalker;

/// Builds the versioned raw vanilla archive from installed game files.
#[derive(Debug, Clone)]
pub struct BuildVanillaArchive {
  /// Installed GTA V Legacy directory containing base RPFs and update/.
  pub game_dir: PathBuf,
  /// Cache directory containing cache_info.json and version directories.
  pub output_dir: PathBuf,
  /// Stop after this cumulative stage; preceding overlays remain in the cache.
  pub through_version: Option<String>,
}

impl BuildVanillaArchive {
  /// Reads installed root RPFs and ordered DLCs, saving changed raw files and version logs.
  pub fn run(
    &self,
    codewalker: &CodeWalker,
  ) -> Result<()> {
    let game_dir = self.game_dir.canonicalize()?;
    let roots = base_archives(&game_dir)?;
    game_path(&game_dir, Path::new("update/update.rpf"))?;
    codewalker.load_game_keys(&game_dir)?;
    let output_dir = reset_output_directory(&self.output_dir, &game_dir)?;
    let staging_path = output_dir.join(format!(".staging-{}", std::process::id()));
    fs::create_dir(&staging_path)?;
    let staging = Staging(staging_path);
    let build = staging.0.join("cache");
    fs::create_dir(&build)?;
    let result = self.build(codewalker, &game_dir, roots, &staging.0, &build, &output_dir);
    if result.is_err() {
      match preserve_failed_logs(&build, &output_dir) {
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
    output_dir: &Path,
  ) -> Result<()> {
    let through_version = self.through_version.as_deref().map(str::to_ascii_lowercase);
    let mut manifest = VanillaCacheManifest {
      format_version: 1,
      game_dir: game_dir.into(),
      versions: vec![],
    };
    let mut current = BTreeMap::new();
    let mut rpf_names = BTreeSet::new();
    let base_dir = scratch.join("base");
    stage(&mut manifest, &mut current, build, "base", || {
      let mut files = Vec::new();
      for relative in &roots {
        let directory = base_dir.join(relative);
        let extracted = extract(codewalker, game_dir, relative, &directory, None, &mut rpf_names)?;
        files.extend(
          extracted
            .into_iter()
            .filter(|file| !file.source.contains("/dlcpacks/"))
            .map(|file| (file, directory.clone())),
        );
      }
      Ok((roots.clone(), files))
    })?;
    fs::remove_dir_all(&base_dir)?;
    if through_version.as_deref() == Some("base") {
      return self.publish_cache(build, output_dir, &manifest, &rpf_names);
    }

    let update_dir = scratch.join("update");
    let mut update = Vec::new();
    let mut dlc_items = Vec::new();
    stage(&mut manifest, &mut current, build, "update", || {
      update =
        extract(codewalker, game_dir, "update/update.rpf", &update_dir, None, &mut rpf_names)?;
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
    })?;
    if through_version.as_deref() == Some("update") {
      return self.publish_cache(build, output_dir, &manifest, &rpf_names);
    }

    let dlc_stages = dlc_items
      .into_iter()
      .map(|item| -> Result<_> {
        let relative = dlc_archive(&item)?.to_string_lossy().replace('\\', "/");
        let label = Path::new(&relative)
          .parent()
          .and_then(Path::file_name)
          .and_then(|name| name.to_str())
          .ok_or("DLC name is missing")?
          .to_owned();
        Ok((item, relative, label))
      })
      .collect::<Result<Vec<_>>>()?;
    if let Some(requested) = through_version.as_deref()
      && !dlc_stages.iter().any(|(_, _, label)| label.eq_ignore_ascii_case(requested))
    {
      return Err(format!("Unknown DLC stage requested: {requested}").into());
    }

    let mut platform_index = None;
    for (item, relative, label) in dlc_stages {
      let stop_after =
        through_version.as_deref().is_some_and(|requested| label.eq_ignore_ascii_case(requested));
      let directory = scratch.join("dlc");
      stage(&mut manifest, &mut current, build, &label, || {
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
        let extracted =
          extract(codewalker, game_dir, &physical, &directory, subtree.as_deref(), &mut rpf_names)?;
        let mut files: Vec<_> =
          extracted.into_iter().map(|file| (file, directory.clone())).collect();
        let patches: Vec<_> = update
          .iter()
          .filter(|file| patch_dlc(&file.source) == Some(label.as_str()))
          .cloned()
          .collect();
        let mut archives = vec![subtree.unwrap_or(physical)];
        if !patches.is_empty() {
          archives.push(format!("update/update.rpf/dlc_patch/{label}"));
          files.extend(patches.into_iter().map(|file| (file, update_dir.clone())));
        }
        Ok((archives, files))
      })?;
      fs::remove_dir_all(directory)?;
      if stop_after {
        break;
      }
    }
    self.publish_cache(build, output_dir, &manifest, &rpf_names)
  }

  fn publish_cache(
    &self,
    build: &Path,
    output_dir: &Path,
    manifest: &VanillaCacheManifest,
    rpf_names: &BTreeSet<String>,
  ) -> Result<()> {
    write_json(&build.join("rpf_names.json"), rpf_names)?;
    write_json(&build.join("cache_info.json"), &manifest)?;
    write_hash_index(build, manifest, rpf_names)?;
    publish_cache(build, output_dir, manifest)?;
    log::info!("Saved {} vanilla stages to {}", manifest.versions.len(), output_dir.display());
    Ok(())
  }
}
