use std::{
  collections::BTreeMap,
  fs,
  io::{self, BufReader},
  path::{Path, PathBuf},
  time::UNIX_EPOCH,
};

use quick_xml::{Reader, events::Event};
use serde::{Deserialize, Serialize};

use crate::{
  core::common::function::collect_files_with_suffix,
  core::merge::ymap_metadata_diff::reference_hash,
};

const CACHE_VERSION: u32 = 1;
const CACHE_FILE_NAME: &str = ".vanilla_ymap_parent_cache.json";

#[derive(Default, Serialize, Deserialize)]
struct CacheFile {
  version: u32,
  entries: BTreeMap<PathBuf, CacheEntry>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
struct CacheEntry {
  modified_seconds: u64,
  modified_nanos: u32,
  parent_hash: u32,
}

pub(crate) struct VanillaParentCache {
  entries: BTreeMap<PathBuf, CacheEntry>,
}

pub(crate) struct CacheUpdateStats {
  pub(crate) parsed: usize,
  pub(crate) reused: usize,
  pub(crate) removed: usize,
}

impl VanillaParentCache {
  /// Updates the parent index, reparsing only files whose modification time changed.
  pub(crate) fn update(vanilla_dir: &Path) -> io::Result<(Self, CacheUpdateStats)> {
    let cache_dir =
      vanilla_dir.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or(vanilla_dir);
    let cache_path = cache_dir.join(CACHE_FILE_NAME);
    let mut cached = match fs::read(&cache_path) {
      Ok(bytes) => match serde_json::from_slice::<CacheFile>(&bytes) {
        Ok(cache) if cache.version == CACHE_VERSION => cache.entries,
        Ok(_) => BTreeMap::new(),
        Err(error) => {
          log::warn!("Ignoring invalid YMAP parent cache {}: {error}", cache_path.display());
          BTreeMap::new()
        }
      },
      Err(error) if error.kind() == io::ErrorKind::NotFound => BTreeMap::new(),
      Err(error) => return Err(error),
    };

    let mut entries = BTreeMap::new();
    let mut parsed = 0;
    let mut reused = 0;
    for path in collect_files_with_suffix(vanilla_dir, ".ymap.xml") {
      let relative_path = path
        .strip_prefix(vanilla_dir)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?
        .to_path_buf();
      let metadata = fs::metadata(&path)?;
      let modified = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
      let modified_seconds = modified.as_secs();
      let modified_nanos = modified.subsec_nanos();
      let entry = match cached.remove(&relative_path) {
        Some(entry)
          if entry.modified_seconds == modified_seconds
            && entry.modified_nanos == modified_nanos =>
        {
          reused += 1;
          entry
        }
        _ => {
          parsed += 1;
          CacheEntry {
            modified_seconds,
            modified_nanos,
            parent_hash: read_parent_hash(&path)?,
          }
        }
      };
      entries.insert(relative_path, entry);
    }

    let stats = CacheUpdateStats {
      parsed,
      reused,
      removed: cached.len(),
    };
    let cache = CacheFile {
      version: CACHE_VERSION,
      entries: entries.clone(),
    };
    fs::write(cache_path, serde_json::to_vec(&cache).map_err(io::Error::other)?)?;
    log::info!(
      "Vanilla YMAP parent cache: parsed {}, reused {}, removed {}",
      stats.parsed,
      stats.reused,
      stats.removed
    );
    Ok((
      Self {
        entries,
      },
      stats,
    ))
  }

  pub(crate) fn paths<'a>(
    &'a self,
    vanilla_dir: &'a Path,
  ) -> impl Iterator<Item = PathBuf> + 'a {
    self.entries.keys().map(move |path| vanilla_dir.join(path))
  }

  pub(crate) fn children(
    &self,
    vanilla_dir: &Path,
    parents: &std::collections::HashSet<u32>,
  ) -> Vec<(u32, PathBuf)> {
    self
      .entries
      .iter()
      .filter(|(_, entry)| parents.contains(&entry.parent_hash))
      .map(|(relative_path, _)| {
        let name = relative_path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        let name = name.strip_suffix(".xml").unwrap_or(name);
        let name = name.strip_suffix(".ymap").unwrap_or(name);
        (reference_hash(name), vanilla_dir.join(relative_path))
      })
      .collect()
  }
}

fn read_parent_hash(path: &Path) -> io::Result<u32> {
  let mut reader = Reader::from_reader(BufReader::new(fs::File::open(path)?));
  reader.config_mut().trim_text(true);
  let mut buffer = Vec::new();
  let mut inside_parent = false;
  loop {
    match reader
      .read_event_into(&mut buffer)
      .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
    {
      Event::Start(element) if element.name().as_ref() == b"parent" => inside_parent = true,
      Event::Text(text) if inside_parent => {
        let text =
          text.decode().map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        return Ok(reference_hash(&text));
      }
      Event::CData(text) if inside_parent => {
        let text =
          text.decode().map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        return Ok(reference_hash(&text));
      }
      Event::End(element) if element.name().as_ref() == b"parent" => return Ok(0),
      Event::Empty(element) if element.name().as_ref() == b"parent" => return Ok(0),
      Event::Start(element) if element.name().as_ref() == b"entities" => return Ok(0),
      Event::Eof => return Ok(0),
      _ => {}
    }
    buffer.clear();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn cache_reuses_unchanged_vanilla_files() {
    let staging = std::env::temp_dir().join(format!("ymap_parent_cache_{}", std::process::id()));
    let vanilla_dir = staging.join("vanilla/ymap.xml");
    fs::create_dir_all(&vanilla_dir).unwrap();
    fs::write(vanilla_dir.join("child.ymap.xml"), "<CMapData><parent>parent</parent></CMapData>")
      .unwrap();

    let (_, initial) = VanillaParentCache::update(&vanilla_dir).unwrap();
    assert_eq!(initial.parsed, 1);
    assert_eq!(initial.reused, 0);
    let (cache, next) = VanillaParentCache::update(&vanilla_dir).unwrap();
    assert_eq!(next.parsed, 0);
    assert_eq!(next.reused, 1);
    assert_eq!(
      cache.children(&vanilla_dir, &[reference_hash("parent")].into_iter().collect()).len(),
      1
    );
    assert!(staging.join("vanilla/.vanilla_ymap_parent_cache.json").is_file());

    fs::remove_dir_all(staging).unwrap();
  }
}
