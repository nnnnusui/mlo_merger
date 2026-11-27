//! Coordinates base archives, title updates and ordered DLC stages.

use super::{
  archives::{
    DlcList, ExtractedFile, RpfNames, base_archives, dlc_archive, dlc_content, dlc_subpack,
    extract, extract_from, game_path, patch_dlc, platform_archives, sub_pack_count,
  },
  content::FileSelection,
  manifest::{FORMAT_VERSION, VanillaCacheManifest},
  publication::{Staging, publish_cache, reset_output_directory},
  stage::{Cumulative, PackFiles, PatchSource, stage_pack, stage_with},
};
use crate::common::json::write_json;
use crate::core::config::gamebuild::UpdateRpfs;
use crate::core::format::rpf::RpfKeys;
use crate::platform::fs;
use crate::platform::process;
use sha1::{Digest, Sha1};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Builds the versioned raw archive from installed game files.
#[derive(Debug, Clone)]
pub(crate) struct BuildGameArchive {
  /// Installed GTA V Legacy directory containing base RPFs and update/.
  pub(crate) game_dir: PathBuf,
  /// Cache directory containing cache_info.json and version directories.
  pub(crate) output_dir: PathBuf,
  /// CodeWalker's `magic.dat`, decoded with the key in GTA5.exe.
  pub(crate) magic: PathBuf,
  /// FiveM's `game-storage` directory, holding the game builds' `update/update.rpf`, from which
  /// each build's title-update patches of the DLC packs are read; without it, only the
  /// installed game's are.
  pub(crate) game_storage: Option<PathBuf>,
  /// The content sets extracted into this archive.
  pub(crate) files: FileSelection,
}

/// The title updates the DLC packs' patches are read from.
struct TitleUpdates {
  /// The game builds they were read for.
  builds: BTreeSet<u32>,
  /// The title update read for each build ([`VanillaCacheManifest::title_updates`]).
  read_from: BTreeMap<u32, String>,
  /// Each distinct title update's `dlc_patch/` files, with the builds reading it.
  sources: Vec<TitleUpdateSource>,
}

struct TitleUpdateSource {
  builds: Vec<u32>,
  /// The builds of [`Self::builds`] whose own title update it is.
  own: Vec<u32>,
  installed: bool,
  files: Vec<ExtractedFile>,
  directory: PathBuf,
}

/// The FiveM game-storage name prefixes of a build's `update/update.rpf`, followed by its SHA-1.
const STORAGE_UPDATE_RPF: [&str; 2] = ["update+update.rpf_", "override+update+update.rpf_"];

impl BuildGameArchive {
  /// Reads installed root RPFs and ordered DLCs, saving changed raw files by version.
  pub(crate) fn run_with_metadata(
    &self,
    metadata_files: &[&str],
    obsolete_files: &[&str],
    write_metadata: impl FnOnce(&Path, &VanillaCacheManifest, &RpfNames) -> Result<()>,
  ) -> Result<()> {
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
    self.build(
      &keys,
      &game_dir,
      roots,
      &staging.0,
      &build,
      &output_dir,
      metadata_files,
      obsolete_files,
      write_metadata,
    )
  }

  #[allow(clippy::too_many_arguments)]
  fn build(
    &self,
    keys: &RpfKeys,
    game_dir: &Path,
    roots: Vec<String>,
    scratch: &Path,
    build: &Path,
    output_dir: &Path,
    metadata_files: &[&str],
    obsolete_files: &[&str],
    write_metadata: impl FnOnce(&Path, &VanillaCacheManifest, &RpfNames) -> Result<()>,
  ) -> Result<()> {
    let extensions = &self.files.extensions();
    let mut manifest = VanillaCacheManifest {
      format_version: FORMAT_VERSION,
      contents: self.files.contents.clone(),
      game_dir: game_dir.into(),
      title_updates: BTreeMap::new(),
      versions: vec![],
    };
    let mut current = Cumulative::default();
    let mut rpf_names = RpfNames::default();
    let base_dir = scratch.join("base");
    stage_with(&mut manifest, &mut current, build, "base", extensions, || {
      let mut files = Vec::new();
      for relative in &roots {
        let directory = base_dir.join(relative);
        let extracted =
          extract(keys, game_dir, relative, &directory, None, &mut rpf_names, &self.files)?;
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
    stage_with(&mut manifest, &mut current, build, "update", extensions, || {
      update = extract(
        keys,
        game_dir,
        "update/update.rpf",
        &update_dir,
        None,
        &mut rpf_names,
        &self.files,
      )?;
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

    let title_updates =
      self.title_updates(keys, game_dir, scratch, &mut rpf_names, (&update, &update_dir))?;
    manifest.title_updates = title_updates.read_from.clone();
    let mut platform_index = None;
    for (item, relative, label) in dlc_stages {
      let directory = scratch.join("dlc");
      stage_pack(
        &mut manifest,
        &mut current,
        build,
        &label,
        extensions,
        &title_updates.builds,
        || {
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
          let extracted = extract(
            keys,
            game_dir,
            &physical,
            &directory,
            subtree.as_deref(),
            &mut rpf_names,
            &self.files,
          )?;
          let patches =
            pack_patches(&label, extensions, &title_updates.sources, (&extracted, &directory))?;
          let content = dlc_content(&label, (&extracted, &directory), (&[], &directory))?;
          // Subpacks are mounted with the pack, after its dlc.rpf, as CodeWalker does.
          let sub_packs =
            if subtree.is_none() { sub_pack_count(&extracted, &directory)? } else { 0 };
          let mut files: Vec<_> =
            extracted.into_iter().map(|file| (file, directory.clone())).collect();
          let mut sub_pack_archives = Vec::new();
          for index in 1..=sub_packs {
            let sub_pack = dlc_subpack(&physical, index);
            if game_path(game_dir, Path::new(&sub_pack)).is_err() {
              tracing::warn!("{physical} declares {sub_packs} subpacks, but {sub_pack} is missing");
              continue;
            }
            let sub_directory = directory.join(format!("dlc{index}"));
            let extracted = extract(
              keys,
              game_dir,
              &sub_pack,
              &sub_directory,
              None,
              &mut rpf_names,
              &self.files,
            )?;
            files.extend(extracted.into_iter().map(|file| (file, sub_directory.clone())));
            sub_pack_archives.push(sub_pack);
          }
          let mut archives = vec![subtree.unwrap_or(physical)];
          archives.extend(sub_pack_archives);
          Ok(PackFiles {
            archives,
            files,
            dlc: content,
            patches,
          })
        },
      )?;
      fs::remove_dir_all(directory)?;
    }
    Self::publish_cache(
      build,
      output_dir,
      &manifest,
      &rpf_names,
      metadata_files,
      obsolete_files,
      write_metadata,
    )
  }

  /// The title updates each game build of `[update_rpf]` reads the DLC packs' patches from:
  /// FiveM's copy of the build's `update/update.rpf` in [`Self::game_storage`], else of the
  /// closest build below it that has one, else of the closest above, else the installed game's
  /// (`update`, extracted into the directory given with it). Without a game-storage directory,
  /// only the installed game's.
  fn title_updates(
    &self,
    keys: &RpfKeys,
    game_dir: &Path,
    scratch: &Path,
    rpf_names: &mut RpfNames,
    (update, update_dir): (&[ExtractedFile], &Path),
  ) -> Result<TitleUpdates> {
    let installed_patches = || update.iter().filter(|file| patch_dlc(&file.source).is_some());
    let installed_only = || TitleUpdates {
      builds: BTreeSet::new(),
      read_from: BTreeMap::new(),
      sources: vec![TitleUpdateSource {
        builds: Vec::new(),
        own: Vec::new(),
        installed: true,
        files: installed_patches().cloned().collect(),
        directory: update_dir.to_path_buf(),
      }],
    };
    let Some(storage) = &self.game_storage else {
      return Ok(installed_only());
    };
    let configured = UpdateRpfs::load(&crate::core::config::path())?.builds;
    if configured.is_empty() {
      tracing::warn!(
        "[update_rpf] in config.toml lists no game builds; reading the title-update patches of the installed game only"
      );
      return Ok(installed_only());
    }
    let mut stored = BTreeMap::new();
    for entry in fs::read_dir(storage)
      .map_err(|error| format!("Cannot read game storage {}: {error}", storage.display()))?
    {
      let entry = entry?;
      let name = entry.file_name().to_string_lossy().into_owned();
      if let Some(sha1) = STORAGE_UPDATE_RPF.iter().find_map(|prefix| name.strip_prefix(prefix)) {
        stored.entry(sha1.to_ascii_lowercase()).or_insert_with(|| entry.path());
      }
    }
    let installed_path = game_path(game_dir, Path::new("update/update.rpf"))?;
    tracing::info!("Hashing {}", installed_path.display());
    let installed = sha1_file(&installed_path)?;
    let available = configured
      .iter()
      .filter(|(_, sha1)| **sha1 == installed || stored.contains_key(*sha1))
      .map(|(build, _)| *build)
      .collect::<BTreeSet<_>>();
    let mut readers = BTreeMap::<String, Vec<u32>>::new();
    let own = |sha1: &str| {
      configured.iter().filter(|(_, own)| *own == sha1).map(|(build, _)| *build).collect()
    };
    let mut read_from = BTreeMap::new();
    for (&build, sha1) in &configured {
      let sha1 = if available.contains(&build) {
        sha1.clone()
      } else if let Some(&other) =
        available.range(..build).next_back().or_else(|| available.range(build..).next())
      {
        tracing::warn!(
          "update.rpf of game build {build} ({sha1}) is not in {}; reading build {other}'s",
          storage.display()
        );
        configured[&other].clone()
      } else {
        tracing::warn!(
          "update.rpf of game build {build} ({sha1}) is not in {}; reading the installed game's",
          storage.display()
        );
        installed.clone()
      };
      read_from.insert(
        build,
        if sha1 == installed {
          "update/update.rpf".to_owned()
        } else {
          let path: &Path = &stored[&sha1];
          path.file_name().map_or(sha1.clone(), |name| name.to_string_lossy().into_owned())
        },
      );
      readers.entry(sha1).or_default().push(build);
    }
    let mut sources = vec![TitleUpdateSource {
      builds: readers.remove(&installed).unwrap_or_default(),
      own: own(&installed),
      installed: true,
      files: installed_patches().cloned().collect(),
      directory: update_dir.to_path_buf(),
    }];
    for (sha1, builds) in readers {
      let directory = scratch.join("title_updates").join(&sha1);
      let files = extract_from(
        keys,
        &stored[&sha1],
        "update/update.rpf",
        &directory,
        Some("update.rpf/dlc_patch"),
        rpf_names,
        &self.files,
      )?;
      sources.push(TitleUpdateSource {
        builds,
        own: own(&sha1),
        installed: false,
        files,
        directory,
      });
    }
    Ok(TitleUpdates {
      builds: configured.keys().copied().collect(),
      read_from,
      sources,
    })
  }

  fn publish_cache(
    build: &Path,
    output_dir: &Path,
    manifest: &VanillaCacheManifest,
    rpf_names: &RpfNames,
    metadata_files: &[&str],
    obsolete_files: &[&str],
    write_metadata: impl FnOnce(&Path, &VanillaCacheManifest, &RpfNames) -> Result<()>,
  ) -> Result<()> {
    write_json(&super::manifest_path(build), manifest)?;
    write_metadata(build, manifest, rpf_names)?;
    tracing::info!("Publishing {} vanilla stages", manifest.versions.len());
    publish_cache(build, output_dir, manifest, metadata_files, obsolete_files)?;
    tracing::info!("Saved {} vanilla stages to {}", manifest.versions.len(), output_dir.display());
    Ok(())
  }
}

/// The versions of the title update's patch of the DLC pack `pack` among `sources`, each with the
/// builds shipping it and what the pack declares with it, the pack's own files, extracted from
/// its `dlc.rpf` into the directory given with them, standing for those the patch lacks.
///
/// Patches holding the same files of the selected `extensions` and declaring the same are one
/// version, as the files they differ in are not extracted.
fn pack_patches(
  pack: &str,
  extensions: &[&str],
  sources: &[TitleUpdateSource],
  (extracted, directory): (&[ExtractedFile], &Path),
) -> Result<Vec<PatchSource>> {
  struct Version {
    key: (Vec<(String, String)>, Option<String>),
    own: Vec<u32>,
    patch: PatchSource,
  }
  let mut versions: Vec<Version> = Vec::new();
  for source in sources {
    let files = source
      .files
      .iter()
      .filter(|file| patch_dlc(&file.source) == Some(pack))
      .cloned()
      .collect::<Vec<_>>();
    if files.is_empty() {
      continue;
    }
    let dlc = dlc_content(pack, (extracted, directory), (&files, &source.directory))?;
    let mut streamed = files
      .iter()
      .filter(|file| {
        file.name.rsplit_once('.').is_some_and(|(_, extension)| extensions.contains(&extension))
      })
      .map(|file| (file.source.clone(), file.sha256.clone()))
      .collect::<Vec<_>>();
    streamed.sort();
    let key = (streamed, dlc.as_ref().map(serde_json::to_string).transpose()?);
    if let Some(version) = versions.iter_mut().find(|version| version.key == key) {
      version.patch.builds.extend(&source.builds);
      version.patch.builds.sort_unstable();
      version.own.extend(&source.own);
      version.patch.installed |= source.installed;
      continue;
    }
    versions.push(Version {
      key,
      own: source.own.clone(),
      patch: PatchSource {
        name: String::new(),
        builds: source.builds.clone(),
        installed: source.installed,
        files: files.into_iter().map(|file| (file, source.directory.clone())).collect(),
        dlc,
      },
    });
  }
  let mut patches = versions
    .into_iter()
    .map(|version| PatchSource {
      name: version
        .own
        .iter()
        .min()
        .map_or(super::stage::INSTALLED_PATCH.to_owned(), u32::to_string),
      ..version.patch
    })
    .collect::<Vec<_>>();
  patches.sort_by_key(|patch| (patch.builds.first().copied().unwrap_or(u32::MAX), patch.installed));
  Ok(patches)
}

/// The lowercase hex SHA-1 of the file at `path`, as FiveM names its copies of game files.
fn sha1_file(path: &Path) -> Result<String> {
  let mut file = fs::File::open(path)?;
  let mut hasher = Sha1::new();
  let mut buffer = vec![0; 1 << 20];
  loop {
    let read = file.read(&mut buffer)?;
    if read == 0 {
      break;
    }
    hasher.update(&buffer[..read]);
  }
  Ok(hex::encode(hasher.finalize()))
}
