//! The merged files a search reads and, with `--all`, the inputs the merge recorded.

use super::{FindEntity, Result};
use crate::core::format::{stream_file::StreamFile, ymap::model::Ymap};
use crate::platform::fs;
use globset::{GlobBuilder, GlobMatcher};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
  collections::{BTreeMap, BTreeSet, HashMap},
  path::{Component, Path, PathBuf},
};

/// Where `--all` reads the inputs a merge recorded, which it names relative to these stores.
#[derive(Debug, Clone)]
pub struct InputStores {
  /// Source resources, holding sources that were not moved into the source cache.
  pub source_dir: PathBuf,
  pub source_cache_dir: PathBuf,
  pub vanilla_cache_dir: PathBuf,
}

impl Default for InputStores {
  fn default() -> Self {
    Self {
      source_dir: "asset/source".into(),
      source_cache_dir: "asset/source-cache".into(),
      vanilla_cache_dir: "asset/vanilla-cache".into(),
    }
  }
}

impl InputStores {
  /// Where a recorded merge input is: a vanilla file in the vanilla cache, a source in the
  /// source root when it is recorded at its original path, otherwise in the source cache.
  pub(super) fn resolve(
    &self,
    input: &mut Input,
  ) {
    let store = match &input.original_path {
      None => &self.vanilla_cache_dir,
      Some(original) if *original == input.path => &self.source_dir,
      Some(_) => &self.source_cache_dir,
    };
    input.path = store.join(&input.path);
  }
}

#[derive(Deserialize)]
pub(super) struct MergeCache {
  pub(super) format_version: u32,
  pub(super) files: BTreeMap<String, Record>,
}
#[derive(Deserialize)]
pub(super) struct Record {
  pub(super) merge_sources: Vec<Input>,
  pub(super) vanilla: Option<Input>,
}
#[derive(Clone, Deserialize)]
pub(super) struct Input {
  pub(super) path: PathBuf,
  pub(super) resource: Option<String>,
  pub(super) original_path: Option<PathBuf>,
  pub(super) fingerprint: InputFingerprint,
}
#[derive(Clone, Deserialize)]
pub(super) struct InputFingerprint {
  pub(super) sha256: String,
  pub(super) size: u64,
}

/// The merged files by lowercase name, the merge metadata with `--all`, and every name to search.
pub(super) type SearchInputs = (BTreeMap<String, PathBuf>, Option<MergeCache>, BTreeSet<String>);

pub(super) fn search_inputs(
  search: &FindEntity,
  extension: &str,
) -> Result<SearchInputs> {
  let merged = fs::canonicalize(&search.merged_dir)?;
  let filter = filename_filter(search.filter.as_deref())?;
  let mut paths = BTreeMap::new();
  for entry in crate::platform::fs::WalkDir::new(&merged).follow_links(false) {
    let entry = entry?;
    if !entry.file_type().is_file() {
      continue;
    }
    let path = entry.path();
    if path.extension().is_none_or(|value| !value.eq_ignore_ascii_case(extension)) {
      continue;
    }
    let name = path
      .file_name()
      .and_then(|name| name.to_str())
      .ok_or("Resource filename is not UTF-8")?
      .to_ascii_lowercase();
    if matches(&filter, &name) && paths.insert(name.clone(), path.to_path_buf()).is_some() {
      return Err(format!("Duplicate merged {extension} filename: {name}").into());
    }
  }
  let cache = if search.diff_all {
    let mut cache: MergeCache =
      crate::common::json::read_json(crate::core::merge::cache_info_path(&merged))?;
    if cache.format_version != 1 {
      return Err("Unsupported merge cache schema for --diff-all".into());
    }
    for record in cache.files.values_mut() {
      for input in record.vanilla.iter_mut().chain(&mut record.merge_sources) {
        search.stores.resolve(input);
      }
    }
    Some(cache)
  } else {
    None
  };
  let mut names = paths.keys().cloned().collect::<BTreeSet<_>>();
  if let Some(cache) = &cache {
    names.extend(
      cache
        .files
        .keys()
        .filter(|name| {
          Path::new(name).extension().is_some_and(|value| value.eq_ignore_ascii_case(extension))
            && matches(&filter, name)
        })
        .cloned(),
    );
  }
  Ok((paths, cache, names))
}

pub(super) fn filename_filter(pattern: Option<&str>) -> Result<Option<GlobMatcher>> {
  pattern
    .map(|pattern| {
      if pattern.contains(['/', '\\'])
        || Path::new(pattern).components().count() != 1
        || !matches!(Path::new(pattern).components().next(), Some(Component::Normal(_)))
      {
        return Err("--filter requires a filename glob, not a path".into());
      }
      Ok(
        GlobBuilder::new(pattern)
          .case_insensitive(true)
          .literal_separator(true)
          .build()?
          .compile_matcher(),
      )
    })
    .transpose()
}

pub(super) fn matches(
  filter: &Option<GlobMatcher>,
  name: &str,
) -> bool {
  filter.as_ref().is_none_or(|filter| filter.is_match(name))
}

/// Reads a stream file, naming the stage and path when it cannot be read or decoded.
pub(super) fn read_stream(
  path: &Path,
  stage: &str,
) -> Result<StreamFile> {
  StreamFile::read(path, &HashMap::new())
    .map_err(|error| format!("Cannot read {stage} resource {}: {error}", path.display()).into())
}

pub(super) fn read_ymap(
  path: &Path,
  stage: &str,
) -> Result<Ymap> {
  match read_stream(path, stage)? {
    StreamFile::Ymap(model) => Ok(model),
    _ => Err(format!("{stage} resource is not a YMAP: {}", path.display()).into()),
  }
}

/// The bytes of `path`, rejecting a recorded input whose contents changed since the merge.
pub(super) fn read_input(
  path: &Path,
  stage: &str,
  input: Option<&Input>,
) -> Result<Vec<u8>> {
  let bytes = fs::read(path).map_err(|error| {
    std::io::Error::new(
      error.kind(),
      format!("Cannot read {stage} resource {}: {error}", path.display()),
    )
  })?;
  if let Some(input) = input
    && (input.fingerprint.size != bytes.len() as u64
      || input.fingerprint.sha256 != hex::encode(Sha256::digest(&bytes)))
  {
    return Err(
      format!(
        "Pre-merge input changed since merge: {}; rerun merge before using --diff-all",
        path.display()
      )
      .into(),
    );
  }
  Ok(bytes)
}
