//! User edits applied to merge inputs without changing source or cache files.
//!
//! `<overwrite>/resources/<resource name>/<stream path>.diff.json` holds a
//! [`StreamFileOverwrite`]: the differences of that stream file's diff document to leave out of
//! the merge, or that the file is left out whole. One leaving out differences applies only while
//! the source file and the diff document's baseline are those it was written against.
//!
//! `<overwrite>/vanilla/latest/ymap/<file name>.diff.json` holds a [`VanillaOverwrite`]:
//! differences from a latest vanilla YMAP, merged after every resource's. It applies only while
//! that vanilla file is the one it was written against.
//!
//! What these call for is derived into the overwrite cache (see [`crate::core::overwrite_cache`]).

mod skip;
#[cfg(test)]
mod test;
mod vanilla;

use crate::common::Result;
use crate::core::format::ymap::compare::YmapDiffFields;
use crate::platform::fs;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

pub use skip::skip_diffs;
pub use vanilla::VanillaOverwrite;

/// The formats, by extension, whose stream files overwrites apply to.
pub const SUPPORTED_FORMATS: &[&str] = &[".ymap"];

/// Whether overwrites apply to the stream file at `stream_path`, by its extension.
pub fn supports(stream_path: &str) -> bool {
  let stream_path = stream_path.to_ascii_lowercase();
  SUPPORTED_FORMATS.iter().any(|extension| stream_path.ends_with(extension))
}

/// The extension of a resource overwrite document, after the stream file's own path.
const DOCUMENT_EXTENSION: &str = ".diff.json";

/// The overwrite of one stream file of a resource, read from
/// `resources/<resource name>/<stream path>.diff.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct StreamFileOverwrite {
  /// Version of the document layout.
  pub schema_version: u32,
  /// Leaves the stream file out of the merge whole, whatever it holds; `source`, `vanilla` and
  /// `skip` are then not needed.
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub ignore: bool,
  /// The source file this overwrite was written against.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub source: Option<OverwriteFingerprint>,
  /// The baseline of the diff document this overwrite was written against, its `vanilla`: the
  /// latest vanilla file, the empty YMAP a YMAP resources add is compared with, or the base
  /// source's YMAP.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub vanilla: Option<OverwriteFingerprint>,
  /// The differences to leave out of the merge, in the shape of the diff document's `diffs`.
  ///
  /// In an object, `true` leaves out the field's whole difference and a nested selector only
  /// the parts it names. In an array, each selector leaves out the items it matches: an item's
  /// variant matches when every field written equals the item's, and its `changes`, when given,
  /// leaves out only the changes it names instead of the whole item.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional, type = Option<YmapDiffSkip>))]
  pub skip: Option<serde_json::Value>,
}

impl StreamFileOverwrite {
  /// The current [`Self::schema_version`].
  pub const SCHEMA_VERSION: u32 = 1;
}

/// An overwrite the merge leaves out, as the files it names are missing or differ from those it
/// was written against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct IgnoredOverwrite {
  /// The resource of a resource overwrite, in lowercase; absent for a vanilla overwrite.
  #[serde(skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  #[serde(default)]
  pub resource: Option<String>,
  /// The overwritten file, in lowercase: a resource overwrite's stream path, such as
  /// `vanilla/hei_ch1_11.ymap`, or the vanilla overwrite's file name, such as `hei_ch1_11.ymap`.
  pub file: String,
  pub reason: IgnoredOverwriteReason,
}

/// Why an overwrite is left out of the merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum IgnoredOverwriteReason {
  /// The overwrite's file is of a format overwrites do not support yet (see
  /// [`SUPPORTED_FORMATS`]).
  UnsupportedFormat,
  /// No merged source stream file is the resource overwrite's file.
  MissingSource,
  /// The source file differs from the one the overwrite was written against.
  SourceChanged,
  /// No latest vanilla file has the overwrite's file name.
  MissingVanilla,
  /// The latest vanilla file differs from the one the overwrite was written against; reported
  /// only when the source, if any, is unchanged.
  VanillaChanged,
}

/// The content of a file an overwrite was written against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct OverwriteFingerprint {
  pub sha256: String,
  pub size: u64,
}

impl From<&crate::core::format::ymap::compare::YmapComparisonFile> for OverwriteFingerprint {
  fn from(file: &crate::core::format::ymap::compare::YmapComparisonFile) -> Self {
    Self {
      sha256: file.sha256.clone(),
      size: file.size,
    }
  }
}

impl OverwriteFingerprint {
  /// The empty YMAP the diffs of the YMAPs resources add are taken against.
  pub fn empty_baseline() -> Result<Self> {
    Ok(Self::from(&crate::core::source_cache::empty_baseline_file()?))
  }

  /// The content of the file at `path`, after resolving links as latest vanilla files are.
  pub fn read(path: &Path) -> Result<Self> {
    let fingerprint =
      crate::common::fingerprint::Fingerprint::read(&fs::canonicalize(path)?, None, false)?;
    Ok(Self {
      sha256: fingerprint.sha256,
      size: fingerprint.size,
    })
  }
}

impl StreamFileOverwrite {
  /// Why this overwrite does not apply to the current `source` file and diff baseline
  /// `vanilla`, if it does not. One leaving the file out whole always applies.
  pub fn ignored_because(
    &self,
    source: &OverwriteFingerprint,
    vanilla: &OverwriteFingerprint,
  ) -> Option<IgnoredOverwriteReason> {
    if self.ignore {
      None
    } else if self.source.as_ref() != Some(source) {
      Some(IgnoredOverwriteReason::SourceChanged)
    } else if self.vanilla.as_ref() != Some(vanilla) {
      Some(IgnoredOverwriteReason::VanillaChanged)
    } else {
      None
    }
  }

  /// A digest of this overwrite, for telling whether a merge used the current one.
  pub fn digest(&self) -> String {
    hex::encode(Sha256::digest(serde_json::to_vec(self).unwrap_or_default()))
  }

  /// Leaves this overwrite's skipped differences out of `diff`, the differences of the source
  /// from its baseline as they are now; does nothing when they are not the files it was written
  /// against, or when it leaves the file out whole, which the merge does instead. `label` names
  /// the stream file in the log.
  pub fn apply(
    &self,
    diff: &mut YmapDiffFields,
    source: &OverwriteFingerprint,
    vanilla: &OverwriteFingerprint,
    label: &str,
  ) -> Result<()> {
    let Some(skip) = self.skip.as_ref().filter(|_| !self.ignore) else {
      return Ok(());
    };
    // `Merge` reports this with every other overwrite it leaves out.
    if self.ignored_because(source, vanilla).is_some() {
      tracing::debug!("Overwrite of {label} was written against other files; not applied");
      return Ok(());
    }
    let unmatched = skip_diffs(diff, skip).map_err(|error| format!("{label}: {error}"))?;
    for path in unmatched {
      tracing::warn!("Overwrite of {label} skips {path}, which is not among the differences");
    }
    Ok(())
  }
}

/// Every resource stream file's overwrite, keyed by resource name and stream path, and the
/// latest vanilla files'.
#[derive(Debug, Clone, Default)]
pub struct Overwrites {
  resources: BTreeMap<(String, String), StreamFileOverwrite>,
  /// Where each of [`Self::resources`] was read from.
  paths: BTreeMap<(String, String), std::path::PathBuf>,
  vanilla: BTreeMap<String, VanillaOverwrite>,
}

impl Overwrites {
  /// Reads `<directory>/resources/<resource>/**/*.diff.json` and
  /// `<directory>/vanilla/latest/ymap/*.diff.json`; a missing directory has no overwrites.
  pub fn load(directory: &Path) -> Result<Self> {
    let (resources, paths) = load_resources(directory)?;
    Ok(Self {
      resources,
      paths,
      vanilla: vanilla::load(directory)?,
    })
  }

  /// The overwrite of the stream file at `stream_path` below the `stream` directory of the
  /// resource named `resource`, matched case-insensitively.
  pub fn stream_file(
    &self,
    resource: &str,
    stream_path: &str,
  ) -> Option<&StreamFileOverwrite> {
    self.resources.get(&(resource.to_ascii_lowercase(), normalize(stream_path)))
  }

  /// The overwrite of `source`, a source stream file the source cache lists.
  pub fn for_source(
    &self,
    source: &crate::core::source_cache::MergeSourceFile,
  ) -> Option<&StreamFileOverwrite> {
    self.resources.get(&source.stream_key())
  }

  /// The file the overwrite of that stream file was read from.
  pub fn stream_file_path(
    &self,
    resource: &str,
    stream_path: &str,
  ) -> Option<&Path> {
    self
      .paths
      .get(&(resource.to_ascii_lowercase(), normalize(stream_path)))
      .map(|path| path.as_path())
  }

  /// Every overwritten stream file as `(resource name, stream path)`, in lowercase.
  pub fn keys(&self) -> impl Iterator<Item = (&str, &str)> {
    self.resources.keys().map(|(resource, path)| (resource.as_str(), path.as_str()))
  }

  /// The overwrite of the latest vanilla file named `file_name`, matched case-insensitively.
  pub fn vanilla_file(
    &self,
    file_name: &str,
  ) -> Option<&VanillaOverwrite> {
    self.vanilla.get(&file_name.to_ascii_lowercase())
  }

  /// Every latest vanilla file's overwrite, keyed by lowercase file name.
  pub fn vanilla_files(&self) -> &BTreeMap<String, VanillaOverwrite> {
    &self.vanilla
  }
}

/// Reads `<directory>/resources/<resource>/**/*.diff.json`, keyed by lowercase resource name and
/// stream path. Files directly in `resources`, the earlier one-document-per-resource layout, are
/// reported and not read.
type ResourceOverwrites =
  (BTreeMap<(String, String), StreamFileOverwrite>, BTreeMap<(String, String), std::path::PathBuf>);

fn load_resources(directory: &Path) -> Result<ResourceOverwrites> {
  let resources_dir = directory.join("resources");
  let mut overwrites = BTreeMap::new();
  let mut paths = BTreeMap::new();
  if !fs::is_dir(&resources_dir) {
    return Ok((overwrites, paths));
  }
  for entry in crate::platform::fs::WalkDir::new(&resources_dir).follow_links(false) {
    let entry = entry?;
    if !entry.file_type().is_file() {
      continue;
    }
    let path = entry.path();
    let relative = path.strip_prefix(&resources_dir)?.to_string_lossy().replace('\\', "/");
    let Some((resource, file)) = relative.split_once('/') else {
      tracing::warn!(
        "Resource overwrites are read from resources/<resource>/<stream path>.diff.json; ignoring {}",
        path.display()
      );
      continue;
    };
    let Some(stream_path) = file.strip_suffix(DOCUMENT_EXTENSION) else {
      tracing::warn!("Not a resource overwrite; ignoring {}", path.display());
      continue;
    };
    let overwrite: StreamFileOverwrite = crate::common::json::read_json(path)
      .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    if overwrite.schema_version != StreamFileOverwrite::SCHEMA_VERSION {
      return Err(
        format!(
          "{} has schema version {}; expected {}",
          path.display(),
          overwrite.schema_version,
          StreamFileOverwrite::SCHEMA_VERSION
        )
        .into(),
      );
    }
    if !overwrite.ignore
      && (overwrite.source.is_none() || overwrite.vanilla.is_none() || overwrite.skip.is_none())
    {
      return Err(
        format!("{} needs source, vanilla and skip unless it sets ignore", path.display()).into(),
      );
    }
    if !supports(stream_path) {
      tracing::warn!(
        "Overwrites of {} files are not supported yet; {} is not applied",
        Path::new(stream_path).extension().map_or("extensionless".into(), |extension| {
          format!(".{}", extension.to_string_lossy())
        }),
        path.display()
      );
    }
    let key = (resource.to_ascii_lowercase(), normalize(stream_path));
    paths.insert(key.clone(), path.to_path_buf());
    overwrites.insert(key, overwrite);
  }
  tracing::info!(
    "Loaded overwrites of {} stream files from {}",
    overwrites.len(),
    resources_dir.display()
  );
  Ok((overwrites, paths))
}

fn normalize(stream_path: &str) -> String {
  crate::common::path::stream_key(stream_path)
}

/// The TypeScript type of [`StreamFileOverwrite::skip`], written out by
/// [`crate::typescript::export_types`] as [`YMAP_DIFF_SKIP_TS`].
#[cfg(feature = "typescript")]
pub(crate) struct YmapDiffSkip;

#[cfg(feature = "typescript")]
impl specta::Type for YmapDiffSkip {
  fn inline(
    _: specta::DefOpts,
    _: &[specta::DataType],
  ) -> std::result::Result<specta::DataType, specta::ExportError> {
    Ok(specta::DataType::Reference(specta::DataTypeReference {
      name: "YmapDiffSkip",
      sid: specta::sid!(@with_specta_path; "YmapDiffSkip"; specta),
      generics: Vec::new(),
    }))
  }
}

/// The TypeScript definitions of [`YmapDiffSkip`], which Specta cannot derive.
#[cfg(feature = "typescript")]
pub(crate) const YMAP_DIFF_SKIP_TS: &str = r#"
/** The differences of a `YmapDiffFields` document to leave out of a merge. */
export type YmapDiffSkip = DiffSkip<YmapDiffFields>

/**
 * In an object, `true` leaves out the whole difference and a nested selector only the parts it
 * names; in an array, each selector leaves out the items it matches.
 */
export type DiffSkip<T> =
  | true
  | (T extends (infer I)[] ? ItemSkip<I>[] : T extends object ? { [K in keyof T]?: DiffSkip<NonNullable<T[K]>> } : never)

/**
 * Matches an array item: every field written must equal the item's, except `changes`, which
 * leaves out only the changes it names instead of the whole item.
 */
export type ItemSkip<I> = I extends object
  ? {
      [V in keyof I]?: I[V] extends { changes: infer C }
        ? DiffMatch<Omit<I[V], "changes">> & { changes?: DiffSkip<NonNullable<C>> }
        : DiffMatch<I[V]>
    }
  : I

/** A value an item's field must equal; objects match on the fields written. */
export type DiffMatch<T> = T extends (infer I)[] ? I[] : T extends object ? { [K in keyof T]?: DiffMatch<T[K]> } : T
"#;
