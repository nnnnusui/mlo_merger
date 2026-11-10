//! What the overwrites call for, derived from them, the source cache and the vanilla cache.
//!
//! The overwrite cache is to the overwrites what the source cache is to the sources: read-side
//! preprocessing, rebuilt from them and nothing else, which the merge reads. It holds:
//!
//! - `added_ymaps.json` ([`AddedYmaps`]): for each YMAP that resources add, which resource's YMAP
//!   wins and which other resources' differences from it are merged into it.
//! - `diff/{resource}/{stream path}.diff.json`: for a YMAP whose base source an
//!   [`AddedYmapOverwrite`](crate::core::overwrite::AddedYmapOverwrite) names, each other
//!   resource's differences from the base source's YMAP. Resource overwrites written against them
//!   apply as against source-cache diff documents.
//! - `base/{resource}/{stream path}.diff.json`: the base source's own resource overwrite, written
//!   against the empty YMAP, as removals from its YMAP, merged as the first source.
//! - `warnings.json` ([`OverwriteWarnings`]): added YMAPs no overwrite names the base source of,
//!   added YMAP overwrites whose base source does not ship the YMAP, and overwrites that no
//!   longer match their files. The merge logs them on every run.
//!
//! Without an overwrite naming its base source, a YMAP that several resources add is taken from
//! the resource whose name sorts last, as the one loaded last, with a warning. A stream file
//! whose resource overwrite sets `ignore` is left out of the choice and of the merge.

use crate::common::json::{read_json, write_json};
use crate::core::config::matching;
use crate::core::format::ymap::compare::{
  StreamFileRef, YmapComparisonContext, YmapComparisonDiff, YmapDiffFields,
};
use crate::core::overwrite::{OverwriteFingerprint, Overwrites};
use crate::core::source_cache::{MergeInputs, MergeSourceFile};
use crate::platform::fs;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

mod warnings;

pub use warnings::{
  AddedYmapWithoutBase, OverwriteWarnings, UnmatchedBaseSource, collect as warnings,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// File name of the [`AddedYmaps`] record.
const ADDED_YMAPS: &str = "added_ymaps.json";
/// The directory of the other sources' diffs from their base sources.
const DIFF_DIR: &str = "diff";
/// The directory of the base sources' overwrites as removals.
const BASE_DIR: &str = "base";
/// File name of the [`OverwriteWarnings`].
const WARNINGS: &str = "warnings.json";
/// Bump when what the cache holds or what [`AddedYmaps::inputs`] covers changes.
const FORMAT_VERSION: u32 = 2;

/// Which resource's YMAP wins for each YMAP that resources add (`added_ymaps.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct AddedYmaps {
  pub format_version: u32,
  /// A digest of what the cache was built from: the source and vanilla caches' inventories,
  /// every overwrite document and the matching tolerances.
  pub inputs: String,
  /// Keyed by lowercase file name.
  pub files: BTreeMap<String, AddedYmap>,
}

/// How one YMAP that resources add is merged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct AddedYmap {
  /// The stream file whose YMAP wins.
  pub base: AddedYmapSource,
  pub chosen_by: AddedYmapChoice,
  /// The names of every resource shipping the YMAP, ignored ones included. When there are
  /// several, the merged YMAP is always written, so that it replaces theirs.
  pub shipped_by: Vec<String>,
  /// The other stream files whose differences from the base are merged into it, each with its
  /// diff document; only when an overwrite names the base source.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub sources: Vec<AddedYmapSource>,
  /// With [`Self::sources`], the base source's overwrite as removals from its YMAP, relative to
  /// the cache.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub base_diff: Option<String>,
}

/// One stream file of a YMAP that resources add.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct AddedYmapSource {
  /// The resource's ID, its path relative to the source root.
  pub resource: String,
  /// The path below the resource's `stream` directory.
  pub stream_path: String,
  /// Its differences from the base source's YMAP, relative to the cache.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub diff: Option<String>,
}

/// Why the base of an added YMAP won.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum AddedYmapChoice {
  /// Its resource is the only one shipping the YMAP.
  Only,
  /// Its resource's name sorts last of those shipping the YMAP, as the one loaded last.
  LastLoaded,
  /// An added YMAP overwrite names it.
  Overwrite,
}

/// Builds or updates the overwrite cache.
#[derive(Debug, Clone)]
pub struct BuildOverwriteCache {
  pub source_dir: PathBuf,
  pub source_cache_dir: PathBuf,
  pub vanilla_cache_dir: PathBuf,
  /// The overwrites; `None` has none.
  pub overwrite_dir: Option<PathBuf>,
  pub output_dir: PathBuf,
}

impl BuildOverwriteCache {
  /// Brings the cache up to date with the overwrites and the source and vanilla caches, which
  /// must already be built; returns whether anything was written. Diffs whose files are
  /// unchanged are kept, and documents nothing calls for any more are removed.
  pub fn run(&self) -> Result<bool> {
    matching::with_config(|| self.run_inner())?
  }

  fn run_inner(&self) -> Result<bool> {
    let source_dir = fs::canonicalize(&self.source_dir)?;
    let source_cache_dir = fs::canonicalize(&self.source_cache_dir)?;
    let vanilla_cache_dir = fs::canonicalize(&self.vanilla_cache_dir)?;
    let output = &self.output_dir;
    let inputs_digest =
      inputs_digest(&source_cache_dir, &vanilla_cache_dir, self.overwrite_dir.as_deref())?;
    if read_json::<AddedYmaps>(output.join(ADDED_YMAPS)).is_ok_and(|previous| {
      previous.format_version == FORMAT_VERSION && previous.inputs == inputs_digest
    }) && read_json::<OverwriteWarnings>(output.join(WARNINGS)).is_ok()
    {
      tracing::info!("Overwrite cache is current; nothing to do");
      return Ok(false);
    }
    let inputs = crate::core::source_cache::load_merge_inputs(&source_cache_dir, &source_dir)?;
    let overwrites = match &self.overwrite_dir {
      Some(directory) => Overwrites::load(directory)?,
      None => Overwrites::default(),
    };
    let mut files = resolve(&inputs, &overwrites);
    let mut written = BTreeSet::new();
    if files.values().any(|file| !file.sources.is_empty()) {
      let context = YmapComparisonContext::new(
        &source_dir,
        &source_cache_dir,
        &vanilla_cache_dir,
        Default::default(),
      )?;
      let sources = inputs
        .ymap
        .iter()
        .map(|source| ((source.resource.clone(), normalize(&source.stream_path)), source))
        .collect::<BTreeMap<_, _>>();
      let tolerances = matching::current();
      for (name, file) in files.iter_mut().filter(|(_, file)| !file.sources.is_empty()) {
        let base = sources[&(file.base.resource.clone(), normalize(&file.base.stream_path))];
        let base_ref = stream_file(&context, base);
        // The base source's overwrite, written against the empty YMAP, as removals from its YMAP.
        let document = base_document(&context, base, &base_ref, &overwrites, name)?;
        let relative = document_path(BASE_DIR, base);
        write_if_changed(&output.join(&relative), &document)?;
        written.insert(output.join(&relative));
        file.base_diff = Some(relative);
        for other in &mut file.sources {
          let source = sources[&(other.resource.clone(), normalize(&other.stream_path))];
          let reference = stream_file(&context, source);
          let relative = document_path(DIFF_DIR, source);
          let path = output.join(&relative);
          let current = read_json::<YmapComparisonDiff>(&path).is_ok_and(|document| {
            document.schema_version == YmapComparisonDiff::SCHEMA_VERSION
              && document.tolerances == tolerances
              && context
                .is_diff_current_against(
                  &reference,
                  &base_ref,
                  &document.source,
                  &document.vanilla,
                  None,
                )
                .unwrap_or(false)
          });
          if !current {
            let document = context.get_diff_against(&reference, &base_ref)?;
            fs::create_dir_all(path.parent().ok_or("Diff path has no parent")?)?;
            write_json(&path, &document)?;
          }
          written.insert(path);
          other.diff = Some(relative);
        }
      }
    }
    for directory in [DIFF_DIR, BASE_DIR] {
      remove_unwritten(&output.join(directory), &written)?;
    }
    let warnings = warnings::collect(&inputs, &overwrites, &files, &vanilla_cache_dir)?;
    fs::create_dir_all(output)?;
    write_json(&output.join(WARNINGS), &warnings)?;
    write_json(
      &output.join(ADDED_YMAPS),
      &AddedYmaps {
        format_version: FORMAT_VERSION,
        inputs: inputs_digest,
        files,
      },
    )?;
    tracing::info!("Wrote the overwrite cache to {}", output.display());
    if !warnings.is_empty() {
      tracing::info!(
        "{} lists {} added YMAPs without a base source, {} unmatched base sources and {} ignored overwrites",
        output.join(WARNINGS).display(),
        warnings.added_ymaps_without_base.len(),
        warnings.unmatched_base_sources.len(),
        warnings.ignored_overwrites.len()
      );
    }
    Ok(true)
  }
}

/// The added YMAPs and warnings the overwrite cache at `cache_dir` records, which must have been
/// built from the current source and vanilla caches and overwrites.
pub fn load(
  cache_dir: &Path,
  source_cache_dir: &Path,
  vanilla_cache_dir: &Path,
  overwrite_dir: Option<&Path>,
) -> Result<(AddedYmaps, OverwriteWarnings)> {
  let path = cache_dir.join(ADDED_YMAPS);
  let added = read_json::<AddedYmaps>(&path).map_err(|error| {
    format!("Cannot read {}: {error}; build the overwrite cache", path.display())
  })?;
  let current = added.format_version == FORMAT_VERSION
    && added.inputs == inputs_digest(source_cache_dir, vanilla_cache_dir, overwrite_dir)?;
  if !current {
    return Err(
      format!(
        "{} is stale; build the overwrite cache (--generate-overwrite-cache)",
        path.display()
      )
      .into(),
    );
  }
  let path = cache_dir.join(WARNINGS);
  let warnings = read_json::<OverwriteWarnings>(&path).map_err(|error| {
    format!("Cannot read {}: {error}; build the overwrite cache", path.display())
  })?;
  Ok((added, warnings))
}

/// The added YMAPs of `inputs` without an overwrite cache: each one's winner, by default or as
/// `overwrites` choose. An overwrite naming a base source needs the cache's diffs, so it is an
/// error.
pub fn resolve_without_cache(
  inputs: &MergeInputs,
  overwrites: &Overwrites,
) -> Result<BTreeMap<String, AddedYmap>> {
  let files = resolve(inputs, overwrites);
  if files.values().any(|file| !file.sources.is_empty()) {
    return Err("Base sources of added YMAPs need an overwrite cache directory".into());
  }
  Ok(files)
}

/// Chooses each added YMAP's winner among the sources of `inputs` that no overwrite ignores; see
/// the module docs. The sources merged into a base an overwrite names have no diff yet.
fn resolve(
  inputs: &MergeInputs,
  overwrites: &Overwrites,
) -> BTreeMap<String, AddedYmap> {
  let mut shippers = BTreeMap::<String, Vec<&MergeSourceFile>>::new();
  for source in &inputs.ymap {
    let name = source.file_name.to_ascii_lowercase();
    if inputs.added_ymaps.contains(&name) {
      shippers.entry(name).or_default().push(source);
    }
  }
  let mut files = BTreeMap::new();
  for (name, mut sources) in shippers {
    // Resource names are unique, so the last by lowercase name is the last loaded.
    sources.sort_by_key(|source| resource_name(&source.resource).to_ascii_lowercase());
    let shipped_by = sources
      .iter()
      .map(|source| resource_name(&source.resource))
      .collect::<BTreeSet<_>>()
      .into_iter()
      .collect::<Vec<_>>();
    let listed = shipped_by.join(", ");
    let candidates = sources
      .into_iter()
      .filter(|source| {
        let ignored = overwrites
          .stream_file(&resource_name(&source.resource), &source.stream_path)
          .is_some_and(|overwrite| overwrite.ignore);
        if ignored {
          tracing::info!(
            "Added YMAP {name} of {} is ignored by its overwrite",
            resource_name(&source.resource)
          );
        }
        !ignored
      })
      .collect::<Vec<_>>();
    let Some(last) = candidates.last().copied() else {
      continue;
    };
    let named = overwrites.added_file(&name).and_then(|overwrite| {
      let found = candidates
        .iter()
        .find(|source| resource_name(&source.resource).eq_ignore_ascii_case(&overwrite.base_source))
        .copied();
      if found.is_none() {
        tracing::debug!(
          "Overwrite of added YMAP {name} names base source {}, which does not ship it or is ignored; using {}, which loads last of {listed}",
          overwrite.base_source,
          resource_name(&last.resource)
        );
      }
      found
    });
    let entry = |source: &MergeSourceFile| AddedYmapSource {
      resource: source.resource.clone(),
      stream_path: source.stream_path.replace('\\', "/"),
      diff: None,
    };
    let file = match named {
      Some(base) => {
        tracing::info!(
          "Added YMAP {name} is shipped by {listed}; using {} as its base, as its overwrite names",
          resource_name(&base.resource)
        );
        AddedYmap {
          base: entry(base),
          chosen_by: AddedYmapChoice::Overwrite,
          sources: candidates
            .iter()
            .filter(|source| !std::ptr::eq(**source, base))
            .map(|source| entry(source))
            .collect(),
          shipped_by,
          base_diff: None,
        }
      }
      None => {
        let chosen_by = if shipped_by.len() > 1 {
          tracing::debug!(
            "Added YMAP {name} is shipped by {listed}; using {}, which loads last. Name the base source in added/ymap/{name}.json of the overwrites to choose another",
            resource_name(&last.resource)
          );
          AddedYmapChoice::LastLoaded
        } else {
          AddedYmapChoice::Only
        };
        AddedYmap {
          base: entry(last),
          chosen_by,
          shipped_by,
          sources: Vec::new(),
          base_diff: None,
        }
      }
    };
    files.insert(name, file);
  }
  files
}

/// A digest of what the cache is built from; see [`AddedYmaps::inputs`].
fn inputs_digest(
  source_cache_dir: &Path,
  vanilla_cache_dir: &Path,
  overwrite_dir: Option<&Path>,
) -> Result<String> {
  let mut digest = Sha256::new();
  digest.update(FORMAT_VERSION.to_le_bytes());
  digest.update(serde_json::to_vec(&matching::current())?);
  for path in [
    crate::core::source_cache::cache_info_path(source_cache_dir),
    vanilla_cache_dir.join("cache_info.json"),
  ] {
    digest.update(Sha256::digest(fs::read(&path)?));
  }
  if let Some(directory) = overwrite_dir.filter(|directory| fs::is_dir(directory)) {
    let mut files = Vec::new();
    for entry in crate::platform::fs::WalkDir::new(directory).follow_links(false) {
      let entry = entry?;
      if entry.file_type().is_file() {
        files.push(entry.path().to_path_buf());
      }
    }
    files.sort();
    for path in files {
      digest.update(path.strip_prefix(directory)?.to_string_lossy().as_bytes());
      digest.update(Sha256::digest(fs::read(&path)?));
    }
  }
  Ok(hex::encode(digest.finalize()))
}

fn normalize(stream_path: &str) -> String {
  stream_path.replace('\\', "/").to_ascii_lowercase()
}

/// The name of the resource with ID `resource`: its directory name.
fn resource_name(resource: &str) -> String {
  Path::new(resource)
    .file_name()
    .map_or_else(|| resource.to_string(), |name| name.to_string_lossy().into_owned())
}

/// The stream file `source` is to the comparison context.
fn stream_file(
  context: &YmapComparisonContext,
  source: &MergeSourceFile,
) -> StreamFileRef {
  StreamFileRef {
    resource_name: Some(resource_name(&source.resource)),
    vanilla_version: context.vanilla_version().to_string(),
    file_name: source.file_name.clone(),
  }
}

/// Where the document of `source` is kept in `directory` of the overwrite cache, relative to it.
fn document_path(
  directory: &str,
  source: &MergeSourceFile,
) -> String {
  format!(
    "{directory}/{}/{}.diff.json",
    resource_name(&source.resource),
    source.stream_path.replace('\\', "/")
  )
}

/// The overwrite of `base`, the base source of the added YMAP `name`, as a document of removals
/// from its YMAP: the items its source-cache diff, against the empty YMAP, lists as additions and
/// the overwrite leaves out. Without an applying overwrite, the document has no differences.
fn base_document(
  context: &YmapComparisonContext,
  base: &MergeSourceFile,
  base_ref: &StreamFileRef,
  overwrites: &Overwrites,
  name: &str,
) -> Result<YmapComparisonDiff> {
  let file = context.comparison_file_of(base_ref)?;
  let mut document = YmapComparisonDiff {
    schema_version: YmapComparisonDiff::SCHEMA_VERSION,
    source: file.clone(),
    vanilla: file,
    tolerances: matching::current(),
    partner: None,
    diffs: YmapDiffFields::default(),
  };
  let overwrite = Path::new(&base.resource)
    .file_name()
    .and_then(|resource| overwrites.stream_file(&resource.to_string_lossy(), &base.stream_path));
  let Some(overwrite) = overwrite else {
    return Ok(document);
  };
  let diff_path = base.diff_path.as_ref().ok_or("Base source has no diff document")?;
  let added: YmapComparisonDiff = read_json(diff_path)?;
  if !added.source.same_content(&document.source) {
    return Err(
      format!(
        "{} is stale for {}; rebuild the source cache",
        diff_path.display(),
        base.path.display()
      )
      .into(),
    );
  }
  let mut kept = added.diffs.clone();
  overwrite.apply(
    &mut kept,
    &OverwriteFingerprint {
      sha256: added.source.sha256.clone(),
      size: added.source.size,
    },
    &OverwriteFingerprint {
      sha256: added.vanilla.sha256.clone(),
      size: added.vanilla.size,
    },
    name,
  )?;
  let mut unsupported = Vec::new();
  let removals = removals(
    &serde_json::to_value(&added.diffs)?,
    &serde_json::to_value(&kept)?,
    "",
    &mut unsupported,
  )
  .unwrap_or(Value::Object(Default::default()));
  for path in unsupported {
    tracing::warn!(
      "Overwrite of {name} ({}) leaves out {path}, which is kept while it has additional sources",
      resource_name(&base.resource)
    );
  }
  document.diffs = serde_json::from_value(removals)?;
  Ok(document)
}

/// The removals that turn the items `full` adds into those `kept` adds: for each array of
/// additions, the index of each one `kept` leaves out, which is that item's index in the YMAP
/// they were all added from. What else `kept` leaves out is listed in `unsupported`.
fn removals(
  full: &Value,
  kept: &Value,
  path: &str,
  unsupported: &mut Vec<String>,
) -> Option<Value> {
  match (full, kept) {
    (Value::Array(full), kept @ (Value::Array(_) | Value::Null)) => {
      let kept = kept.as_array().map(Vec::as_slice).unwrap_or_default();
      let mut next = 0;
      let mut removed = Vec::new();
      for (index, item) in full.iter().enumerate() {
        if kept.get(next) == Some(item) {
          next += 1;
        } else if item.get("Added").is_some() {
          removed.push(serde_json::json!({"Removed": {"index": index}}));
        } else {
          unsupported.push(format!("{path}[{index}]"));
        }
      }
      (!removed.is_empty()).then_some(Value::Array(removed))
    }
    (Value::Object(full), kept @ (Value::Object(_) | Value::Null)) => {
      let fields = full
        .iter()
        .filter_map(|(key, value)| {
          let kept = kept.get(key).unwrap_or(&Value::Null);
          Some((key.clone(), removals(value, kept, &format!("{path}/{key}"), unsupported)?))
        })
        .collect::<serde_json::Map<_, _>>();
      (!fields.is_empty()).then_some(Value::Object(fields))
    }
    (full, kept) if full == kept => None,
    _ => {
      if !full.is_null() {
        unsupported.push(path.to_string());
      }
      None
    }
  }
}

/// Writes `document` to `path` unless it already holds it, keeping its modification time.
fn write_if_changed(
  path: &Path,
  document: &YmapComparisonDiff,
) -> Result<()> {
  let bytes = serde_json::to_vec_pretty(document)?;
  if fs::read(path).is_ok_and(|previous| previous == bytes) {
    return Ok(());
  }
  fs::create_dir_all(path.parent().ok_or("Diff path has no parent")?)?;
  fs::write(path, bytes)?;
  Ok(())
}

/// Removes the files below `directory` other than `written`, and the directories left empty.
fn remove_unwritten(
  directory: &Path,
  written: &BTreeSet<PathBuf>,
) -> Result<()> {
  if !fs::is_dir(directory) {
    return Ok(());
  }
  let mut directories = Vec::new();
  for entry in crate::platform::fs::WalkDir::new(directory).follow_links(false) {
    let entry = entry?;
    if entry.file_type().is_dir() {
      directories.push(entry.path().to_path_buf());
    } else if !written.contains(entry.path()) {
      fs::remove_file(entry.path())?;
    }
  }
  for directory in directories.into_iter().rev() {
    if fs::read_dir(&directory)?.next().is_none() {
      fs::remove_dir(&directory)?;
    }
  }
  Ok(())
}
