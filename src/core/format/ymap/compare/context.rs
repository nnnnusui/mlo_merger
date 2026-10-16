use std::{
  collections::{BTreeSet, HashMap, HashSet, hash_map::Entry},
  fs,
  path::{Path, PathBuf},
  rc::Rc,
};

use super::{
  YmapComparisonData, YmapComparisonDiff, YmapComparisonFile, YmapDiffFields, YmapParentRef,
};
use crate::{
  common::{Result, fingerprint::Fingerprint},
  core::{
    config::matching,
    format::ymap::{binary::ymap_to_model, diff::reference_hash, model::Ymap},
    source_cache::{SourceYmapIndex, load_source_ymap_index, read_stream_bytes},
  },
};

/// Identifies one YMAP stream file in the vanilla cache or a source resource.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize)]
pub struct StreamFileRef {
  /// Source resource name (e.g. `cfx-mlo-example`, its directory name), or `None` for vanilla.
  pub resource_name: Option<String>,
  /// Cumulative vanilla cache version the file belongs to.
  pub vanilla_version: String,
  /// File name including the `.ymap` extension.
  pub file_name: String,
}

#[derive(serde::Deserialize)]
struct VanillaCacheInfo {
  latest_version: String,
}

/// Reads YMAPs from the vanilla and source caches on first use and keeps them for reuse.
pub struct YmapComparisonContext {
  vanilla_cache_dir: PathBuf,
  pub(super) vanilla_version: String,
  pub(super) source_index: SourceYmapIndex,
  /// Lowercase YMAP file names in the vanilla cache.
  pub(super) vanilla_file_names: HashSet<String>,
  /// Lowercase YMAP file names in the vanilla and source caches keyed by their reference hash.
  file_names_by_hash: HashMap<u32, BTreeSet<String>>,
  /// Names used to resolve hashes while decoding, e.g. vanilla `hash_names.json`.
  hash_names: HashMap<u32, String>,
  pub(super) cache_by_ref: HashMap<StreamFileRef, Rc<Ymap>>,
  pub(super) prepared_by_ref: HashMap<StreamFileRef, Rc<YmapComparisonData>>,
}

impl YmapComparisonContext {
  pub fn new(
    source_cache_dir: &Path,
    vanilla_cache_dir: &Path,
    hash_names: HashMap<u32, String>,
  ) -> Result<Self> {
    let info: VanillaCacheInfo =
      serde_json::from_slice(&fs::read(vanilla_cache_dir.join("cache_info.json"))?)?;
    let source_index = load_source_ymap_index(source_cache_dir)?;
    let mut file_names_by_hash = HashMap::<u32, BTreeSet<String>>::new();
    let vanilla_file_names = fs::read_dir(vanilla_cache_dir.join("latest/ymap"))?
      .map(|entry| Ok(entry?.file_name().to_string_lossy().to_ascii_lowercase()))
      .collect::<Result<HashSet<_>>>()?;
    let source_names = source_index.paths.keys().map(|(_, file_name)| file_name.clone());
    for file_name in vanilla_file_names.iter().cloned().chain(source_names) {
      if let Some(stem) = file_name.strip_suffix(".ymap") {
        file_names_by_hash.entry(reference_hash(stem)).or_default().insert(file_name);
      }
    }
    Ok(Self {
      vanilla_cache_dir: vanilla_cache_dir.to_path_buf(),
      vanilla_version: info.latest_version,
      source_index,
      vanilla_file_names,
      file_names_by_hash,
      hash_names,
      cache_by_ref: HashMap::new(),
      prepared_by_ref: HashMap::new(),
    })
  }

  /// Returns the decoded YMAP for `file`, reading it on first access.
  ///
  /// The YMAP is shared with the cache, so callers can hold several at once.
  pub fn get_cache(
    &mut self,
    file: &StreamFileRef,
  ) -> Result<Rc<Ymap>> {
    match self.cache_by_ref.entry(file.clone()) {
      Entry::Occupied(entry) => Ok(entry.get().clone()),
      Entry::Vacant(entry) => {
        let path = resolve_stream_file(
          &self.vanilla_cache_dir,
          &self.vanilla_version,
          &self.source_index,
          file,
        )?;
        let ymap = ymap_to_model(&read_stream_bytes(&path)?, &self.hash_names)?;
        Ok(entry.insert(Rc::new(ymap)).clone())
      }
    }
  }

  /// Returns the comparison data for `file`, deriving it from [`Self::get_cache`] on first
  /// access.
  pub fn get_prepared(
    &mut self,
    file: &StreamFileRef,
  ) -> Result<Rc<YmapComparisonData>> {
    if !self.prepared_by_ref.contains_key(file) {
      let ymap = self.get_cache(file)?;
      let parent_ref = self.parent_ref(file, &ymap.parent)?;
      let data = YmapComparisonData::new(&ymap, parent_ref);
      self.prepared_by_ref.insert(file.clone(), Rc::new(data));
    }
    Ok(self.prepared_by_ref[file].clone())
  }

  /// Returns the differences of `file` from the vanilla YMAP of the same name.
  ///
  /// Tolerances come from the active matching config (`asset/config.toml`).
  pub fn get_diff(
    &mut self,
    file: &StreamFileRef,
  ) -> Result<YmapComparisonDiff> {
    let baseline = StreamFileRef {
      resource_name: None,
      ..file.clone()
    };
    let vanilla = self.get_cache(&baseline)?;
    let modded = self.get_cache(file)?;
    let vanilla_data = self.get_prepared(&baseline)?;
    let modded_data = self.get_prepared(file)?;
    let diffs = YmapDiffFields::between(
      (&vanilla, &vanilla_data),
      (&modded, &modded_data),
      &matching::current(),
    );
    Ok(YmapComparisonDiff {
      source: self.comparison_file(file)?,
      vanilla: self.comparison_file(&baseline)?,
      diffs,
    })
  }

  /// Fingerprints the file that `file` resolves to.
  fn comparison_file(
    &self,
    file: &StreamFileRef,
  ) -> Result<YmapComparisonFile> {
    let path = resolve_stream_file(
      &self.vanilla_cache_dir,
      &self.vanilla_version,
      &self.source_index,
      file,
    )?;
    let Fingerprint {
      sha256,
      size,
      modified_seconds,
      modified_nanos,
    } = Fingerprint::read(&path, None, false)?;
    Ok(YmapComparisonFile {
      path: path.display().to_string(),
      sha256,
      size,
      modified_seconds,
      modified_nanos,
    })
  }

  /// Resolves a declared parent (a name or `hash_XXXXXXXX`) of `child` to a stream file.
  ///
  /// The parent is looked up in the child's resource first and then in vanilla, matching
  /// [`resolve_stream_file`]; parents found only in other resources stay unresolved.
  fn parent_ref(
    &self,
    child: &StreamFileRef,
    parent: &str,
  ) -> Result<Option<YmapParentRef>> {
    let hash = reference_hash(parent);
    if hash == 0 {
      return Ok(None);
    }
    let in_resource = |file_name: &String| {
      child.resource_name.as_ref().filter(|resource| {
        self.source_index.paths.contains_key(&((*resource).clone(), file_name.clone()))
      })
    };
    let candidates = self
      .file_names_by_hash
      .get(&hash)
      .into_iter()
      .flatten()
      .filter(|file_name| {
        in_resource(file_name).is_some() || self.vanilla_file_names.contains(*file_name)
      })
      .collect::<Vec<_>>();
    match candidates.as_slice() {
      [] => Ok(Some(YmapParentRef::Unresolved(hash))),
      [file_name] => Ok(Some(YmapParentRef::Resolved(StreamFileRef {
        resource_name: in_resource(file_name).cloned(),
        vanilla_version: child.vanilla_version.clone(),
        file_name: (*file_name).clone(),
      }))),
      names => Err(
        format!("{} has parent {parent} matching several YMAPs: {names:?}", child.file_name).into(),
      ),
    }
  }
}

/// Locates `file` in its source resource, falling back to vanilla when the resource has no
/// YMAP of that name or `file` names no resource.
///
/// Vanilla cache entries are links into the raw vanilla tree, so their targets are returned.
fn resolve_stream_file(
  vanilla_cache_dir: &Path,
  vanilla_version: &str,
  source_index: &SourceYmapIndex,
  file: &StreamFileRef,
) -> Result<PathBuf> {
  if let Some(resource) = &file.resource_name {
    let key = (resource.clone(), file.file_name.to_ascii_lowercase());
    match source_index.paths.get(&key).map(Vec::as_slice) {
      Some([path]) => {
        if file.vanilla_version != source_index.latest_vanilla_version {
          return Err(
            format!(
              "{resource}/{} requests vanilla version {}, but the source cache was built against {}",
              file.file_name, file.vanilla_version, source_index.latest_vanilla_version
            )
            .into(),
          );
        }
        return Ok(path.clone());
      }
      Some(paths) => {
        return Err(
          format!("{resource} has {} YMAPs named {}", paths.len(), file.file_name).into(),
        );
      }
      None => {}
    }
  }
  if file.vanilla_version != vanilla_version {
    return Err(
      format!(
        "{} requests vanilla version {}, but the vanilla cache holds {vanilla_version}",
        file.file_name, file.vanilla_version
      )
      .into(),
    );
  }
  let path = vanilla_cache_dir.join("latest/ymap").join(&file.file_name);
  if !path.is_file() {
    return Err(format!("No source or vanilla YMAP named {}", file.file_name).into());
  }
  Ok(path.canonicalize()?)
}
