//! Coordinates base archives, title updates and ordered DLC stages.

use crate::platform::fs;
use crate::platform::process;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::{
  Result,
  archives::{
    DlcList, base_archives, dlc_archive, extract, game_path, patch_dlc, platform_archives,
  },
  hash_index::write_hash_index,
  io::write_json,
  manifest::VanillaCacheManifest,
  publication::{Staging, publish_cache, reset_output_directory},
  stage::stage,
};
use crate::core::format::rpf::RpfKeys;

/// Builds the versioned raw vanilla archive from installed game files.
#[derive(Debug, Clone)]
pub struct BuildVanillaArchive {
  /// Installed GTA V Legacy directory containing base RPFs and update/.
  pub game_dir: PathBuf,
  /// Cache directory containing cache_info.json and version directories.
  pub output_dir: PathBuf,
  /// CodeWalker's `magic.dat`, decoded with the key in GTA5.exe.
  pub magic: PathBuf,
}

impl BuildVanillaArchive {
  /// Reads installed root RPFs and ordered DLCs, saving changed raw files by version.
  pub fn run(&self) -> Result<()> {
    let game_dir = fs::canonicalize(&self.game_dir)?;
    let roots = base_archives(&game_dir)?;
    game_path(&game_dir, Path::new("update/update.rpf"))?;
    let executable = game_path(&game_dir, Path::new("GTA5.exe"))
      .map_err(|_| "GTA5.exe is required (GTA V Legacy archives only)")?;
    if !fs::is_file(&self.magic) {
      return Err(format!(
        "magic.dat is missing: {}. Copy CodeWalker.Core/Resources/magic.dat from the CodeWalker sources or specify --magic.",
        self.magic.display()
      )
      .into());
    }
    let keys = RpfKeys::load(&executable, &self.magic)?;
    let output_dir = reset_output_directory(&self.output_dir, &game_dir)?;
    let staging_path = output_dir.join(format!(".staging-{}", process::id()));
    fs::create_dir(&staging_path)?;
    let staging = Staging(staging_path);
    let build = staging.0.join("cache");
    fs::create_dir(&build)?;
    self.build(&keys, &game_dir, roots, &staging.0, &build, &output_dir)
  }

  fn build(
    &self,
    keys: &RpfKeys,
    game_dir: &Path,
    roots: Vec<String>,
    scratch: &Path,
    build: &Path,
    output_dir: &Path,
  ) -> Result<()> {
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
        let extracted = extract(keys, game_dir, relative, &directory, None, &mut rpf_names)?;
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

    let update_dir = scratch.join("update");
    let mut update = Vec::new();
    let mut dlc_items = Vec::new();
    stage(&mut manifest, &mut current, build, "update", || {
      update = extract(keys, game_dir, "update/update.rpf", &update_dir, None, &mut rpf_names)?;
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

    let mut platform_index = None;
    for (item, relative, label) in dlc_stages {
      let directory = scratch.join("dlc");
      stage(&mut manifest, &mut current, build, &label, || {
        let (physical, subtree) =
          if relative.starts_with("x64/") && game_path(game_dir, Path::new(&relative)).is_err() {
            if platform_index.is_none() {
              platform_index = Some(platform_archives(keys, game_dir)?);
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
          extract(keys, game_dir, &physical, &directory, subtree.as_deref(), &mut rpf_names)?;
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
    tracing::info!("Saved {} vanilla stages to {}", manifest.versions.len(), output_dir.display());
    Ok(())
  }
}
