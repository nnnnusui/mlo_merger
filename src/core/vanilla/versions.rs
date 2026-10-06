//! Read-only filename queries over recorded vanilla version changes.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;

use super::{Result, load_manifest};

/// Queries the versions in which a cached vanilla file was introduced or replaced.
#[derive(Debug, Clone)]
pub struct ListVanillaVersions {
  /// Vanilla filename including its extension; matching is case-insensitive.
  pub file_name: String,
  /// GTAV cache root containing root or per-version metadata.
  pub vanilla_cache_dir: PathBuf,
}

/// The kind of recorded file change at a vanilla version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VanillaVersionChange {
  /// The filename first became available.
  Added,
  /// The filename's previously available native content was replaced.
  Modified,
}

/// One version that contains a recorded change for a requested vanilla file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaVersionEntry {
  /// Position-prefixed cache version ID, not a game build number.
  pub version: String,
  /// Whether the file was introduced or replaced at this version.
  pub change: VanillaVersionChange,
  /// Cache-relative path to the native file stored for that stage.
  pub artifact: String,
  /// Original game-relative RPF entry provenance.
  pub source: String,
}

impl ListVanillaVersions {
  /// Lists additions and replacements in cache order, excluding unchanged stages.
  /// Missing filenames produce an empty list; game files are never read.
  ///
  /// ```no_run
  /// let query = mlo_merger::core::vanilla::ListVanillaVersions {
  ///   file_name: "ch1_01.ymap".into(), vanilla_cache_dir: "asset/vanilla-cache".into(),
  /// };
  /// let versions = query.versions()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn versions(&self) -> Result<Vec<VanillaVersionEntry>> {
    let name = self.file_name.trim();
    if name.is_empty() || matches!(name, "." | "..") || name.contains(['/', '\\', ':']) {
      return Err("Expected a vanilla basename such as example.ymap or example.ybn".into());
    }
    let manifest = load_manifest(&self.vanilla_cache_dir)?;
    if manifest.format_version != 1 {
      return Err(format!("Unsupported GTAV cache schema {}", manifest.format_version).into());
    }
    let mut ids = BTreeSet::new();
    let mut previous_version = None;
    let mut previous_hash: Option<&str> = None;
    let mut result = Vec::new();
    for version in &manifest.versions {
      if version.parent.as_deref() != previous_version || !ids.insert(&version.id) {
        return Err(format!("Invalid GTAV version chain at {}", version.id).into());
      }
      previous_version = Some(version.id.as_str());
      for (file_name, change) in &version.changes {
        if !file_name.eq_ignore_ascii_case(name) {
          continue;
        }
        if change.previous_sha256.as_deref() != previous_hash {
          return Err(format!("Invalid content chain for {name} at {}", version.id).into());
        }
        result.push(VanillaVersionEntry {
          version: version.id.clone(),
          change: if change.previous_sha256.is_some() {
            VanillaVersionChange::Modified
          } else {
            VanillaVersionChange::Added
          },
          artifact: change.file.object.clone(),
          source: change.file.source.clone(),
        });
        previous_hash = Some(&change.file.sha256);
      }
    }
    Ok(result)
  }

  /// Writes the ordered change list as a JSON array on stdout without changing the cache.
  pub fn run(&self) -> Result<()> {
    let versions = self.versions()?;
    let mut output = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut output, &versions)?;
    writeln!(output)?;
    output.flush()?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::vanilla::{
    CacheVersion, CachedFile, FileChange, VanillaCacheManifest, write_json,
  };
  use std::collections::BTreeMap;
  use std::fs;

  #[test]
  fn vanilla_version_list_matches_names_and_skips_unchanged_stages() {
    let root = std::env::temp_dir().join(format!("vanilla_version_list_{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let change = |previous: Option<&str>, hash: &str, artifact: &str| FileChange {
      previous_sha256: previous.map(str::to_string),
      file: CachedFile {
        sha256: hash.into(),
        object: artifact.into(),
        source: format!("fixture.rpf/{artifact}"),
      },
    };
    let mut manifest = VanillaCacheManifest {
      format_version: 1,
      game_dir: root.join("missing-game"),
      versions: vec![
        CacheVersion {
          id: "0000-base".into(),
          parent: None,
          archives: vec![],
          unchanged: 0,
          changes: BTreeMap::from([
            ("Example.ymap".into(), change(None, "first", "0000-base/ymap/example.ymap")),
            ("a.ybn".into(), change(None, "ybn-first", "0000-base/ybn/a.ybn")),
          ]),
        },
        CacheVersion {
          id: "0001-unchanged".into(),
          parent: Some("0000-base".into()),
          archives: vec![],
          unchanged: 1,
          changes: BTreeMap::new(),
        },
        CacheVersion {
          id: "0002-patch".into(),
          parent: Some("0001-unchanged".into()),
          archives: vec![],
          unchanged: 0,
          changes: BTreeMap::from([
            (
              "example.ymap".into(),
              change(Some("first"), "second", "0002-patch/ymap/example.ymap"),
            ),
            ("other.ymap".into(), change(None, "other", "0002-patch/ymap/other.ymap")),
            ("a.ybn".into(), change(Some("ybn-first"), "ybn-second", "0002-patch/ybn/a.ybn")),
          ]),
        },
      ],
    };
    write_json(&root.join("cache_info.json"), &manifest).unwrap();
    let mut query = ListVanillaVersions {
      file_name: "EXAMPLE.YMAP".into(),
      vanilla_cache_dir: root.clone(),
    };
    let versions = query.versions().unwrap();
    assert_eq!(
      versions.iter().map(|entry| entry.version.as_str()).collect::<Vec<_>>(),
      ["0000-base", "0002-patch"]
    );
    assert_eq!(versions[0].change, VanillaVersionChange::Added);
    assert_eq!(versions[1].change, VanillaVersionChange::Modified);
    assert!(versions[1].artifact.ends_with(".ymap"));
    query.file_name = "A.YBN".into();
    let bounds = query.versions().unwrap();
    assert_eq!(
      bounds.iter().map(|entry| entry.version.as_str()).collect::<Vec<_>>(),
      ["0000-base", "0002-patch"]
    );
    assert_eq!(bounds[0].change, VanillaVersionChange::Added);
    assert_eq!(bounds[1].change, VanillaVersionChange::Modified);
    assert!(bounds[1].artifact.ends_with(".ybn"));
    query.file_name = "missing.ymap".into();
    assert!(query.versions().unwrap().is_empty());
    for name in ["", ".", "..", "../a.ybn", "folder\\a.ybn"] {
      query.file_name = name.into();
      assert!(query.versions().is_err());
    }
    query.file_name = "../example.ymap".into();
    assert!(query.versions().is_err());
    query.file_name = "example.ymap".into();
    manifest.versions[2].changes.get_mut("example.ymap").unwrap().previous_sha256 =
      Some("wrong".into());
    write_json(&root.join("cache_info.json"), &manifest).unwrap();
    assert!(query.versions().unwrap_err().to_string().contains("Invalid content chain"));
    assert!(!root.join("create_cache.log").exists());
    fs::remove_dir_all(root).unwrap();
  }
}
