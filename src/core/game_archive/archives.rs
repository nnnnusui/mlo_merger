//! RPF extraction and physical/virtual DLC archive resolution.

use crate::platform::fs::{self, File};
use rayon::prelude::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::{
  Result,
  content::{FileClassification, FileSelection},
};
use crate::core::format::rpf::{RpfArchive, RpfFile, RpfKeys};

/// Names collected from RPF table-of-contents entries.
#[derive(Debug, Default)]
pub(crate) struct RpfNames {
  /// Every entry's name stem, as stored and lowercased.
  pub(crate) stems: BTreeSet<String>,
  /// Lowercased `.ydr` and `.ydd` entry names, from which LOD archetype names are derived.
  pub(crate) drawables: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ExtractedFile {
  pub(crate) name: String,
  pub(crate) source: String,
  pub(crate) stored: String,
  pub(crate) sha256: String,
  pub(crate) kind: Option<&'static str>,
}

#[derive(Deserialize)]
pub(crate) struct DlcList {
  #[serde(rename = "Paths")]
  pub(crate) paths: DlcPaths,
}

#[derive(Deserialize)]
pub(crate) struct DlcPaths {
  #[serde(rename = "Item", default)]
  pub(crate) items: Vec<String>,
}

/// A DLC pack's `setup2.xml`, at the root of its `dlc.rpf`.
#[derive(Deserialize)]
struct DlcSetup {
  #[serde(rename = "subPackCount")]
  sub_pack_count: Option<ValueAttribute>,
}

#[derive(Deserialize)]
struct ValueAttribute {
  #[serde(rename = "@value")]
  value: u32,
}

/// How many subpacks (`dlc1.rpf`, `dlc2.rpf`, …) the DLC pack whose `dlc.rpf` `extracted` was
/// taken from has, as its `setup2.xml` declares; `directory` holds the extracted files. Packs
/// such as `mpheist4` keep most of their maps in subpacks, which the game, and CodeWalker,
/// mount with the pack.
pub(crate) fn sub_pack_count(
  extracted: &[ExtractedFile],
  directory: &Path,
) -> Result<u32> {
  let Some(setup) = extracted.iter().find(|file| is_dlc_setup(&file.source)) else {
    return Ok(0);
  };
  let setup: DlcSetup =
    quick_xml::de::from_reader(std::io::BufReader::new(File::open(directory.join(&setup.stored))?))
      .map_err(|error| format!("{}: {error}", setup.source))?;
  Ok(setup.sub_pack_count.map_or(0, |count| count.value))
}

/// Whether `source`, an extracted file's game path, is a DLC pack's `setup2.xml`: at the root of
/// its `dlc.rpf`.
fn is_dlc_setup(source: &str) -> bool {
  source.to_ascii_lowercase().ends_with("/dlc.rpf/setup2.xml")
}

/// The game path of subpack `index` of the DLC pack whose `dlc.rpf` is at `relative`:
/// `dlc{index}.rpf` beside it.
pub(crate) fn dlc_subpack(
  relative: &str,
  index: u32,
) -> String {
  let directory = relative.rsplit_once('/').map_or("", |(directory, _)| directory);
  format!("{directory}/dlc{index}.rpf")
}

pub(crate) fn dlc_archive(item: &str) -> Result<PathBuf> {
  let normalized = item.trim().replace('\\', "/").to_ascii_lowercase();
  let (mount, path) = normalized.split_once(':').ok_or("DLC path has no mount prefix")?;
  let root = match mount {
    "dlcpacks" => "update/x64/dlcpacks",
    "platform" => "x64",
    _ => return Err(format!("Unsupported DLC mount: {item}").into()),
  };
  let path = path.trim_matches('/');
  if path.is_empty() || path.split('/').any(|part| part.is_empty() || part == "." || part == "..") {
    return Err(format!("Invalid DLC path: {item}").into());
  }
  Ok(Path::new(root).join(path).join("dlc.rpf"))
}

/// Resolves a game-relative path case-insensitively on either supported platform.
pub(crate) fn game_path(
  root: &Path,
  relative: &Path,
) -> Result<PathBuf> {
  let mut path = root.to_path_buf();
  for component in relative.iter() {
    let name = component.to_str().ok_or("Game path is not UTF-8")?;
    let matches = fs::read_dir(&path)?
      .map(|entry| entry.map(|entry| entry.path()))
      .collect::<std::io::Result<Vec<_>>>()?
      .into_iter()
      .filter(|candidate| {
        candidate
          .file_name()
          .is_some_and(|value| value.to_string_lossy().eq_ignore_ascii_case(name))
      })
      .collect::<Vec<_>>();
    if matches.len() != 1 {
      return Err(
        format!("Missing or ambiguous game archive path: {}", path.join(name).display()).into(),
      );
    }
    path = matches[0].clone();
  }
  Ok(path)
}

/// Extracts selected extensions, dlclist.xml and a root setup2.xml recursively into `output`, preserving
/// archive provenance, and collects every entry's name into `rpf_names`.
///
/// `subtree` limits extraction to one nested archive's full virtual path. Files are
/// numbered in table-of-contents order, each archive's own files before its children's.
pub(crate) fn extract(
  keys: &RpfKeys,
  game_dir: &Path,
  relative: &str,
  output: &Path,
  subtree: Option<&str>,
  rpf_names: &mut RpfNames,
  selection: &FileSelection,
) -> Result<Vec<ExtractedFile>> {
  tracing::info!("Scanning {relative}");
  let path = game_path(game_dir, Path::new(relative))?;
  extract_from(keys, &path, relative, output, subtree, rpf_names, selection)
}

/// [`extract`] from the archive at `path`, read as the game's archive at `relative`, whose
/// name selects its key and which the extracted files' sources start with.
pub(crate) fn extract_from(
  keys: &RpfKeys,
  path: &Path,
  relative: &str,
  output: &Path,
  subtree: Option<&str>,
  rpf_names: &mut RpfNames,
  selection: &FileSelection,
) -> Result<Vec<ExtractedFile>> {
  let name = Path::new(relative)
    .file_name()
    .and_then(|name| name.to_str())
    .ok_or_else(|| format!("Invalid archive path: {relative}"))?;
  let root = RpfArchive::open_as(path, name, keys)?;
  let prefix = subtree.map(|subtree| format!("{}/", subtree.to_lowercase()));
  let mut selected: Vec<(&RpfArchive, &RpfFile)> = Vec::new();
  for archive in root.archives() {
    for file in &archive.files {
      if prefix.as_ref().is_some_and(|prefix| !file.path.starts_with(prefix)) {
        continue;
      }
      let name = file.name.rsplit_once('.').map_or(file.name.as_str(), |(stem, _)| stem);
      let name = if name.is_empty() { file.name.as_str() } else { name };
      if !name.trim().is_empty() {
        rpf_names.stems.insert(name.to_owned());
        rpf_names.stems.insert(name.to_lowercase());
      }
      let lower = file.name.to_lowercase();
      if lower.ends_with(".ydr") || lower.ends_with(".ydd") {
        rpf_names.drawables.insert(lower);
      }
      if file
        .path
        .rsplit_once('.')
        .is_some_and(|(_, extension)| selection.content(extension).is_some())
        || traversal_metadata(&file.path)
      {
        selected.push((archive, file));
      }
    }
  }

  fs::create_dir_all(output)?;
  selected
    .par_iter()
    .enumerate()
    .map_init(
      || File::open(path),
      |file, (index, (archive, entry))| -> std::result::Result<Option<ExtractedFile>, String> {
        let file = file.as_mut().map_err(|error| format!("{}: {error}", path.display()))?;
        let data = archive
          .extract(file, entry, keys)
          .map_err(|error| format!("Could not extract {}: {error}", entry.path))?;
        let name = entry.name_lower();
        let kind = if traversal_metadata(&entry.path) {
          None
        } else {
          let extension = name.rsplit_once('.').map_or("", |(_, extension)| extension);
          match selection
            .classify(extension, &data)
            .map_err(|error| format!("{}: {error}", entry.path))?
          {
            FileClassification::Excluded => return Ok(None),
            FileClassification::Included {
              kind,
            } => kind,
          }
        };
        let extension = Path::new(&name)
          .extension()
          .map_or(String::new(), |extension| format!(".{}", extension.to_string_lossy()));
        let stored = format!("{index:08}{extension}");
        fs::write(output.join(&stored), &data).map_err(|error| format!("{stored}: {error}"))?;
        let (_, inner) = entry.path.split_once('/').ok_or("Invalid extracted RPF entry path")?;
        Ok(Some(ExtractedFile {
          name,
          source: format!("{relative}/{inner}"),
          stored,
          sha256: hex::encode(Sha256::digest(&data)),
          kind,
        }))
      },
    )
    .collect::<std::result::Result<Vec<_>, _>>()
    .map(|files| files.into_iter().flatten().collect())
    .map_err(Into::into)
}

fn traversal_metadata(path: &str) -> bool {
  let pack_metadata = |path: &str| path.ends_with("/setup2.xml") || path.ends_with("/content.xml");
  // Data files a pack's `content.xml` may declare, such as island hopper files.
  let data_file = |rest: &str| {
    rest.strip_prefix("common/data/").is_some_and(|name| !name.contains('/'))
      && rest.to_ascii_lowercase().ends_with(".meta")
  };
  path.ends_with("/common/data/dlclist.xml")
    // The title update's surface types, which YBN materials name by index.
    || path == "update.rpf/common/data/materials/materials.dat"
    || (path.matches('/').count() == 1 && pack_metadata(path))
    || path.strip_prefix("dlc.rpf/").is_some_and(data_file)
    // The title update's replacements of a pack's, at `dlc_patch/{pack}/`.
    || path.split_once("/dlc_patch/").is_some_and(|(_, rest)| {
      (rest.matches('/').count() == 1 && pack_metadata(&format!("/{rest}")))
        || rest.split_once('/').is_some_and(|(_, rest)| data_file(rest))
    })
}

/// What the DLC pack `pack`, whose `dlc.rpf` `extracted` was taken from into `directory`,
/// declares in its `setup2.xml` and `content.xml`, and the islands its island hopper files in
/// `common/data/` define, each file taken from the title update's replacements at
/// `update/update.rpf/dlc_patch/{pack}/` among `update`, extracted into `update_directory`,
/// when it has them. `None` when `setup2.xml` or `content.xml` is missing.
pub(crate) fn dlc_content(
  pack: &str,
  (extracted, directory): (&[ExtractedFile], &Path),
  (update, update_directory): (&[ExtractedFile], &Path),
) -> Result<Option<super::dlc_content::DlcContent>> {
  let find = |name: &str| {
    let name = name.to_ascii_lowercase();
    let patch = format!("update/update.rpf/dlc_patch/{}/{name}", pack.to_ascii_lowercase());
    update
      .iter()
      .find(|file| file.source.to_ascii_lowercase() == patch)
      .map(|file| (file, update_directory))
      .or_else(|| {
        extracted
          .iter()
          .find(|file| file.source.to_ascii_lowercase().ends_with(&format!("/dlc.rpf/{name}")))
          .map(|file| (file, directory))
      })
  };
  let (Some((setup, setup_dir)), Some((content, content_dir))) =
    (find("setup2.xml"), find("content.xml"))
  else {
    return Ok(None);
  };
  let mut parsed = super::dlc_content::parse(
    &fs::read(setup_dir.join(&setup.stored))?,
    &fs::read(content_dir.join(&content.stored))?,
  )
  .map_err(|error| format!("{}: {error}", content.source))?;
  for file in &parsed.data_files {
    if !file.file_type.eq_ignore_ascii_case(super::dlc_content::ISLAND_HOPPER_FILE) {
      continue;
    }
    let below = file.path.split_once(":/").map_or(file.path.as_str(), |(_, below)| below);
    let Some((hopper, hopper_dir)) = find(below) else {
      tracing::warn!("{pack} declares the island hopper file {}, which is missing", file.path);
      continue;
    };
    let island =
      super::dlc_content::parse_island_hopper(&fs::read(hopper_dir.join(&hopper.stored))?)
        .map_err(|error| format!("{}: {error}", hopper.source))?;
    parsed.island_hoppers.push(island);
  }
  Ok(Some(parsed))
}

pub(crate) fn patch_dlc(source: &str) -> Option<&str> {
  source.split_once("/dlc_patch/")?.1.split('/').next()
}

pub(crate) fn platform_virtual_path(source: &str) -> Option<String> {
  let (archive, path) = source.split_once('/')?;
  if archive.starts_with("x64") && archive.ends_with(".rpf") && path.starts_with("dlcpacks/") {
    Some(format!("x64/{path}"))
  } else {
    None
  }
}

pub(super) fn platform_archives(
  keys: &RpfKeys,
  game_dir: &Path,
) -> Result<BTreeMap<String, (String, String)>> {
  let mut roots = fs::read_dir(game_dir)?
    .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase()))
    .collect::<std::io::Result<Vec<_>>>()?;
  roots.sort();
  let mut archives = BTreeMap::new();
  for relative in roots.into_iter().filter(|name| name.starts_with("x64") && name.ends_with(".rpf"))
  {
    tracing::info!("Indexing platform archives in {relative}");
    let root = RpfArchive::open(&game_path(game_dir, Path::new(&relative))?, keys)?;
    for archive in root.archives() {
      let subtree = archive.path.clone();
      if let Some(virtual_path) = platform_virtual_path(&subtree)
        && archives.insert(virtual_path.clone(), (relative.clone(), subtree)).is_some()
      {
        return Err(format!("Ambiguous platform DLC archive: {virtual_path}").into());
      }
    }
  }
  Ok(archives)
}

pub(crate) fn base_archives(game_dir: &Path) -> Result<Vec<String>> {
  let mut archives = Vec::new();
  for entry in fs::read_dir(game_dir)? {
    let entry = entry?;
    let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
    if entry.file_type()?.is_file() && name.ends_with(".rpf") {
      archives.push(name);
    }
  }
  archives.sort();
  if archives.is_empty() {
    return Err("No base RPF archives found in the game directory".into());
  }
  Ok(archives)
}
