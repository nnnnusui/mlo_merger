//! Validated access to raw vanilla-cache history stages.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::core::{
  format::ybn::{model::Bound, read_ybn},
  vanilla::{CachedFile, VanillaCacheManifest, load_manifest},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn cache_basename(name: &str) -> &str {
  if name.len() >= 4 && name[name.len() - 4..].eq_ignore_ascii_case(".xml") {
    &name[..name.len() - 4]
  } else {
    name
  }
}

/// Validated raw-file history used by pipeline merges.
pub struct VanillaHistory {
  vanilla_root: PathBuf,
  manifest: VanillaCacheManifest,
  latest_files: std::collections::BTreeMap<String, CachedFile>,
  ybn_models: RefCell<HashMap<String, Bound>>,
}

impl VanillaHistory {
  /// Loads the schema-1 vanilla cache and resolves its latest cumulative file set.
  pub fn load(vanilla_root: &Path) -> Result<Self> {
    let vanilla_root = vanilla_root.canonicalize()?;
    let manifest = load_manifest(&vanilla_root)?;
    let latest_version = manifest.versions.last().ok_or("Vanilla cache has no stages")?.id.clone();
    let latest_files = manifest.resolve_version(&latest_version)?;
    Ok(Self {
      vanilla_root,
      manifest,
      latest_files,
      ybn_models: RefCell::new(HashMap::new()),
    })
  }

  pub(super) fn latest_file(
    &self,
    name: &str,
  ) -> Result<&CachedFile> {
    let name = cache_basename(name);
    self
      .latest_files
      .iter()
      .find(|(cached_name, _)| cached_name.eq_ignore_ascii_case(name))
      .map(|(_, file)| file)
      .ok_or_else(|| format!("Latest vanilla cache has no {name}").into())
  }

  /// Returns the YMAP basenames present in the latest cumulative cache stage.
  pub fn ymap_names(&self) -> HashSet<String> {
    self
      .latest_files
      .keys()
      .filter(|name| name.to_ascii_lowercase().ends_with(".ymap"))
      .map(|name| name.to_ascii_lowercase())
      .collect()
  }

  pub(super) fn ymap_files(&self) -> Vec<(String, CachedFile)> {
    self
      .latest_files
      .iter()
      .filter(|(name, _)| name.to_ascii_lowercase().ends_with(".ymap"))
      .map(|(name, file)| (name.clone(), file.clone()))
      .collect()
  }

  pub(super) fn candidate_files(
    &self,
    name: &str,
  ) -> Result<Vec<(String, CachedFile)>> {
    let name = cache_basename(name);
    let mut seen = HashSet::new();
    let mut candidates = Vec::new();
    for version in self.manifest.versions.iter().rev() {
      if let Some((_, change)) =
        version.changes.iter().find(|(cached_name, _)| cached_name.eq_ignore_ascii_case(name))
      {
        let file = &change.file;
        if seen.insert(file.sha256.clone()) {
          candidates.push((version.id.clone(), file.clone()));
        }
      }
    }
    if candidates.is_empty() {
      return Err(format!("Vanilla cache has no history for {name}").into());
    }
    Ok(candidates)
  }

  pub(super) fn ybn_model(
    &self,
    file: &CachedFile,
  ) -> Result<Bound> {
    if let Some(model) = self.ybn_models.borrow().get(&file.sha256) {
      return Ok(model.clone());
    }
    let model = read_ybn(&self.ybn_bytes(file)?)?;
    self.ybn_models.borrow_mut().insert(file.sha256.clone(), model.clone());
    Ok(model)
  }

  pub(super) fn ybn_bytes(
    &self,
    file: &CachedFile,
  ) -> Result<Vec<u8>> {
    let path = self.raw_path(file)?;
    Ok(std::fs::read(path)?)
  }

  pub(super) fn raw_path(
    &self,
    file: &CachedFile,
  ) -> Result<PathBuf> {
    let relative = Path::new(&file.object);
    if file.object.is_empty()
      || file.object.contains(['\\', ':'])
      || relative.components().any(|part| !matches!(part, Component::Normal(_)))
    {
      return Err(format!("Invalid vanilla artifact path: {}", file.object).into());
    }
    let path = self.vanilla_root.join(relative);
    if !path.is_file() {
      return Err(format!("Vanilla cache artifact is missing: {}", path.display()).into());
    }
    let hash = format!("{:x}", Sha256::digest(std::fs::read(&path)?));
    if hash != file.sha256 {
      return Err(format!("Vanilla cache artifact hash mismatch: {}", path.display()).into());
    }
    Ok(path)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::vanilla::{CacheVersion, FileChange, write_json};
  use std::collections::BTreeMap;
  use std::fs;

  #[test]
  fn candidates_are_newest_first_and_duplicate_contents_are_collapsed() {
    let root =
      std::env::temp_dir().join(format!("vanilla_history_candidates_{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let cached = |version: &str, sha256: &str| CachedFile {
      sha256: sha256.into(),
      object: format!("{version}/ymap/map.ymap"),
      source: format!("{version}.rpf/map.ymap"),
    };
    let manifest = VanillaCacheManifest {
      format_version: 1,
      game_dir: "game".into(),
      versions: vec![
        CacheVersion {
          id: "0000-base".into(),
          parent: None,
          archives: vec![],
          changes: BTreeMap::from([(
            "map.ymap".into(),
            FileChange {
              previous_sha256: None,
              file: cached("0000-base", "a"),
            },
          )]),
          unchanged: 0,
        },
        CacheVersion {
          id: "0001-patch".into(),
          parent: Some("0000-base".into()),
          archives: vec![],
          changes: BTreeMap::from([(
            "map.ymap".into(),
            FileChange {
              previous_sha256: Some("a".into()),
              file: cached("0001-patch", "b"),
            },
          )]),
          unchanged: 0,
        },
        CacheVersion {
          id: "0002-unchanged".into(),
          parent: Some("0001-patch".into()),
          archives: vec![],
          changes: BTreeMap::new(),
          unchanged: 1,
        },
        CacheVersion {
          id: "0003-same-content".into(),
          parent: Some("0002-unchanged".into()),
          archives: vec![],
          changes: BTreeMap::from([(
            "map.ymap".into(),
            FileChange {
              previous_sha256: Some("b".into()),
              file: cached("0003-same-content", "b"),
            },
          )]),
          unchanged: 0,
        },
      ],
    };
    write_json(&root.join("cache_info.json"), &manifest).unwrap();

    let history = VanillaHistory::load(&root).unwrap();
    let candidates = history.candidate_files("MAP.YMAP").unwrap();
    assert_eq!(
      candidates.iter().map(|(version, _)| version.as_str()).collect::<Vec<_>>(),
      ["0003-same-content", "0000-base"]
    );
    assert_eq!(
      history
        .candidate_files("MAP.YMAP.XML")
        .unwrap()
        .iter()
        .map(|(version, _)| version.as_str())
        .collect::<Vec<_>>(),
      ["0003-same-content", "0000-base"]
    );
    assert_eq!(history.latest_file("map.ymap").unwrap().sha256, "b");
    assert_eq!(history.ymap_names().len(), 1);
    assert!(history.ymap_names().contains("map.ymap"));
    fs::remove_dir_all(root).unwrap();
  }
}
