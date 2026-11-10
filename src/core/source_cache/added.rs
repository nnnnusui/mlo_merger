//! The stream files resources add, which replace no vanilla file.
//!
//! A stream file no vanilla file is named after is compared, like the others, with a baseline:
//! an empty file of its format, so that everything in it is a difference an overwrite can leave
//! out. The formats whose added files are merged are [`ADDED_FORMATS`]. For a YMAP, the
//! baseline is [`Ymap::empty`], kept in memory, and its name, flags, extents and block are
//! differences too; diff documents record it as [`empty_baseline_file`]. Several resources
//! adding a file of the same name are combined by the merge, as the overwrite cache records (see
//! [`crate::core::overwrite_cache`]).
//!
//! `added_ymap_relationships.json` groups the added YMAPs under the parent each declares, as
//! the vanilla cache's `ymap_relationships.json` does for vanilla YMAPs, naming each added YMAP
//! as `{resource name}/{file name}`, since resources may add YMAPs of the same name.

use super::{Result, read_metadata, types::SourceCacheMetadata};
use crate::common::json::{read_json, write_json};
use crate::core::format::ymap::{
  binary::write_ymap, compare::YmapComparisonFile, model::ymap::Ymap, reference_hash,
};
use crate::platform::fs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
  collections::{BTreeMap, BTreeSet},
  path::Path,
};

/// File name of the relationships of the added YMAPs in the source cache.
pub(crate) const RELATIONSHIPS_FILE: &str = "added_ymap_relationships.json";

/// The parent each added YMAP declares, as the vanilla cache records it for vanilla YMAPs.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AddedYmapRelationships {
  /// The added YMAPs declaring each parent, as `{resource name}/{file name}` in lowercase, keyed
  /// by the parent: another added YMAP of the same resource named that way, else a vanilla
  /// YMAP's lowercase file name, else `hash_XXXXXXXX`.
  pub children_by_parent: BTreeMap<String, Vec<String>>,
}

impl AddedYmapRelationships {
  /// Each parent and child by file name alone, as the merge groups YMAPs.
  pub fn children_by_file_name(&self) -> impl Iterator<Item = (&str, &str)> {
    fn file_name(name: &str) -> &str {
      name.rsplit_once('/').map_or(name, |(_, file)| file)
    }
    self.children_by_parent.iter().flat_map(|(parent, children)| {
      children.iter().map(move |child| (file_name(parent), file_name(child)))
    })
  }
}

/// The formats, by extension, whose stream files resources add are merged.
pub(super) const ADDED_FORMATS: &[&str] = &[".ymap"];

/// Whether `file`, of `format`, is a stream file a resource adds that the merge takes: one no
/// vanilla file is named after, of one of [`ADDED_FORMATS`].
pub(super) fn is_added_stream_file(
  format: &str,
  file: &super::types::SourceFile,
) -> bool {
  ADDED_FORMATS.contains(&format)
    && file.vanilla.is_none()
    && crate::core::stream_file::identifies_stream_file(&file.file_name)
}

/// The encoded [`Ymap::empty`] the added YMAPs are compared with.
pub fn empty_baseline_bytes() -> Result<Vec<u8>> {
  Ok(write_ymap(&Ymap::empty(), &[])?)
}

/// How diff documents record the empty baseline of an added YMAP: no path, and the fingerprint
/// of [`empty_baseline_bytes`].
pub fn empty_baseline_file() -> Result<YmapComparisonFile> {
  let bytes = empty_baseline_bytes()?;
  Ok(YmapComparisonFile {
    path: String::new(),
    sha256: hex::encode(Sha256::digest(&bytes)),
    size: bytes.len() as u64,
    modified_seconds: 0,
    modified_nanos: 0,
  })
}

/// The lowercase file names of the YMAPs some resource adds.
pub(super) fn added_names(metadata: &SourceCacheMetadata) -> BTreeSet<String> {
  metadata
    .resources
    .values()
    .flat_map(|inventory| inventory.files_by_format.get(".ymap").into_iter().flatten())
    .filter(|file| file.cached_path.is_some() && is_added_stream_file(".ymap", file))
    .map(|file| file.file_name.to_ascii_lowercase())
    .collect()
}

/// Writes `added_ymap_relationships.json` for the published inventory, or removes it when no
/// resource adds a YMAP. `vanilla_names` are the lowercase file names of the latest vanilla
/// YMAPs, which added YMAPs may declare as parents. An unchanged file is kept.
pub(super) fn write_added_stream_files(
  output: &Path,
  vanilla_names: &BTreeSet<String>,
) -> Result<()> {
  let metadata = read_metadata(output)?;
  let path = output.join(RELATIONSHIPS_FILE);
  // Earlier versions kept baselines and relationships in `added/`.
  let legacy = output.join("added");
  if fs::is_dir(&legacy) {
    fs::remove_dir_all(&legacy)?;
  }
  // Each resource's added YMAPs and the parents they declare, by lowercase file name.
  let mut resources = BTreeMap::<String, BTreeMap<String, BTreeSet<u32>>>::new();
  for inventory in metadata.resources.values() {
    let resource = inventory
      .source
      .file_name()
      .ok_or("Resource has no name")?
      .to_string_lossy()
      .to_ascii_lowercase();
    for file in inventory.files_by_format.get(".ymap").into_iter().flatten() {
      if file.cached_path.is_none() || !is_added_stream_file(".ymap", file) {
        continue;
      }
      let parent =
        file.ymap_parent_hash.as_ref().and_then(|hash| u32::from_str_radix(hash, 16).ok());
      resources
        .entry(resource.clone())
        .or_default()
        .entry(file.file_name.to_ascii_lowercase())
        .or_default()
        .extend(parent.filter(|hash| *hash != 0));
    }
  }
  if resources.is_empty() {
    return match fs::remove_file(&path) {
      Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
      _ => Ok(()),
    };
  }
  let vanilla = vanilla_names
    .iter()
    .map(|name| (reference_hash(name.strip_suffix(".ymap").unwrap_or(name)), name.clone()))
    .collect::<BTreeMap<_, _>>();
  let mut children_by_parent = BTreeMap::<String, BTreeSet<String>>::new();
  for (resource, files) in &resources {
    let own = files
      .keys()
      .map(|name| (reference_hash(name.strip_suffix(".ymap").unwrap_or(name)), name))
      .collect::<BTreeMap<_, _>>();
    for (child, parents) in files {
      for parent in parents {
        let parent = match (own.get(parent), vanilla.get(parent)) {
          (Some(name), _) if *name != child => format!("{resource}/{name}"),
          (Some(_), _) => continue,
          (None, Some(name)) => name.clone(),
          (None, None) => format!("hash_{parent:08X}"),
        };
        children_by_parent.entry(parent).or_default().insert(format!("{resource}/{child}"));
      }
    }
  }
  let relationships = AddedYmapRelationships {
    children_by_parent: children_by_parent
      .into_iter()
      .map(|(parent, children)| (parent, children.into_iter().collect()))
      .collect(),
  };
  if read_json::<AddedYmapRelationships>(&path).is_ok_and(|previous| previous == relationships) {
    return Ok(());
  }
  write_json(&path, &relationships)
}

/// The relationships of the added YMAPs in the source cache at `cache_dir`; none when it has no
/// added YMAPs.
pub fn added_ymap_relationships(cache_dir: &Path) -> Result<AddedYmapRelationships> {
  let path = cache_dir.join(RELATIONSHIPS_FILE);
  if !fs::is_file(&path) {
    return Ok(AddedYmapRelationships::default());
  }
  read_json(&path).map_err(|error| format!("Cannot read {}: {error}", path.display()).into())
}
