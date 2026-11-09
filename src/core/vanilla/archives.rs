//! RPF extraction and physical/virtual DLC archive resolution.

use crate::platform::fs::{self, File};
use rayon::prelude::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::Result;
use crate::core::format::rpf::{RpfArchive, RpfFile, RpfKeys};

#[derive(Debug, Clone)]
pub(super) struct ExtractedFile {
  pub(super) name: String,
  pub(super) source: String,
  pub(super) stored: String,
  pub(super) sha256: String,
}

#[derive(Deserialize)]
pub(super) struct DlcList {
  #[serde(rename = "Paths")]
  pub(super) paths: DlcPaths,
}

#[derive(Deserialize)]
pub(super) struct DlcPaths {
  #[serde(rename = "Item", default)]
  pub(super) items: Vec<String>,
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
pub(super) fn sub_pack_count(
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
pub(super) fn dlc_subpack(
  relative: &str,
  index: u32,
) -> String {
  let directory = relative.rsplit_once('/').map_or("", |(directory, _)| directory);
  format!("{directory}/dlc{index}.rpf")
}

pub(super) fn dlc_archive(item: &str) -> Result<PathBuf> {
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

/// Extracts YMAPs, YBNs, dlclist.xml and a root setup2.xml recursively into `output`, preserving
/// archive provenance, and collects every entry's name stem into `rpf_names`.
///
/// `subtree` limits extraction to one nested archive's full virtual path. Files are
/// numbered in table-of-contents order, each archive's own files before its children's.
pub(super) fn extract(
  keys: &RpfKeys,
  game_dir: &Path,
  relative: &str,
  output: &Path,
  subtree: Option<&str>,
  rpf_names: &mut BTreeSet<String>,
) -> Result<Vec<ExtractedFile>> {
  tracing::info!("Scanning {relative}");
  let path = game_path(game_dir, Path::new(relative))?;
  let root = RpfArchive::open(&path, keys)?;
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
        rpf_names.insert(name.to_owned());
        rpf_names.insert(name.to_lowercase());
      }
      if file.path.ends_with(".ymap")
        || file.path.ends_with(".ybn")
        || file.path.ends_with("/common/data/dlclist.xml")
        || (file.path.matches('/').count() == 1 && file.path.ends_with("/setup2.xml"))
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
      || File::open(&path),
      |file, (index, (archive, entry))| -> std::result::Result<ExtractedFile, String> {
        let file = file.as_mut().map_err(|error| format!("{}: {error}", path.display()))?;
        let data = archive
          .extract(file, entry, keys)
          .map_err(|error| format!("Could not extract {}: {error}", entry.path))?;
        let name = entry.name_lower();
        let extension = Path::new(&name)
          .extension()
          .map_or(String::new(), |extension| format!(".{}", extension.to_string_lossy()));
        let stored = format!("{index:08}{extension}");
        fs::write(output.join(&stored), &data).map_err(|error| format!("{stored}: {error}"))?;
        let (_, inner) = entry.path.split_once('/').ok_or("Invalid extracted RPF entry path")?;
        Ok(ExtractedFile {
          name,
          source: format!("{relative}/{inner}"),
          stored,
          sha256: hex::encode(Sha256::digest(&data)),
        })
      },
    )
    .collect::<std::result::Result<_, _>>()
    .map_err(Into::into)
}

pub(super) fn patch_dlc(source: &str) -> Option<&str> {
  source.split_once("/dlc_patch/")?.1.split('/').next()
}

pub(super) fn platform_virtual_path(source: &str) -> Option<String> {
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

pub(super) fn base_archives(game_dir: &Path) -> Result<Vec<String>> {
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
