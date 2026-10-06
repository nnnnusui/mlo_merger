//! RPF extraction and physical/virtual DLC archive resolution.

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use super::Result;
use crate::core::codewalker::CodeWalker;

#[derive(Debug, Clone, Deserialize)]
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

pub(super) fn extract(
  codewalker: &CodeWalker,
  game_dir: &Path,
  relative: &str,
  output: &Path,
  subtree: Option<&str>,
  rpf_names: &mut BTreeSet<String>,
) -> Result<Vec<ExtractedFile>> {
  log::info!("Scanning {relative}");
  let archive = game_path(game_dir, Path::new(relative))?;
  match subtree {
    Some(subtree) => codewalker.extract_rpf_subtree(&archive, subtree, output)?,
    None => codewalker.extract_rpf(&archive, output)?,
  }
  let mut files: Vec<ExtractedFile> =
    serde_json::from_reader(BufReader::new(fs::File::open(output.join("files.json"))?))?;
  let names: Vec<String> =
    serde_json::from_reader(BufReader::new(fs::File::open(output.join("rpf_names.json"))?))?;
  rpf_names.extend(names);
  for file in &mut files {
    let (_, entry) = file.source.split_once('/').ok_or("Invalid extracted RPF entry path")?;
    file.source = format!("{relative}/{entry}");
  }
  Ok(files)
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
  codewalker: &CodeWalker,
  game_dir: &Path,
  staging: &Path,
) -> Result<BTreeMap<String, (String, String)>> {
  let mut roots = fs::read_dir(game_dir)?
    .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase()))
    .collect::<std::io::Result<Vec<_>>>()?;
  roots.sort();
  let mut archives = BTreeMap::new();
  for relative in roots.into_iter().filter(|name| name.starts_with("x64") && name.ends_with(".rpf"))
  {
    log::info!("Indexing platform archives in {relative}");
    let output = staging.join("archives.json");
    codewalker.list_rpf_paths(&game_path(game_dir, Path::new(&relative))?, &output)?;
    let paths: Vec<String> = serde_json::from_reader(BufReader::new(fs::File::open(output)?))?;
    for subtree in paths {
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
