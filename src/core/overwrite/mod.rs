//! User edits applied to merge inputs without changing source or cache files.
//!
//! `<overwrite>/resources/<resource name>.json` holds a [`ResourceOverwrite`]: for each stream file
//! of that resource, the differences to leave out of the merge. An entry applies only while the
//! source and vanilla files are those it was written against.
//!
//! `<overwrite>/vanilla/latest/ymap/<file name>.diff.json` holds a [`VanillaOverwrite`]:
//! differences from a latest vanilla YMAP, merged after every resource's. It applies only while
//! that vanilla file is the one it was written against.

mod skip;
#[cfg(test)]
mod test;
mod vanilla;

use crate::common::Result;
use crate::core::format::ymap::compare::YmapDiffFields;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::BufReader;
use std::path::Path;

pub use skip::skip_diffs;
pub use vanilla::VanillaOverwrite;

/// The overwrites of one resource, read from `resources/<resource name>.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct ResourceOverwrite {
  /// Version of the document layout.
  pub schema_version: u32,
  /// The overwrites of each stream file, keyed by its path below the resource's `stream`
  /// directory, such as `vanilla/hei_ch1_11.ymap`.
  pub files: BTreeMap<String, StreamFileOverwrite>,
}

impl ResourceOverwrite {
  /// The current [`Self::schema_version`].
  pub const SCHEMA_VERSION: u32 = 1;
}

/// The overwrites of one stream file of a resource.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct StreamFileOverwrite {
  /// The source file this overwrite was written against.
  pub source: OverwriteFingerprint,
  /// The vanilla file this overwrite was written against.
  pub vanilla: OverwriteFingerprint,
  /// The differences to leave out of the merge, in the shape of the diff document's `diffs`.
  ///
  /// In an object, `true` leaves out the field's whole difference and a nested selector only
  /// the parts it names. In an array, each selector leaves out the items it matches: an item's
  /// variant matches when every field written equals the item's, and its `changes`, when given,
  /// leaves out only the changes it names instead of the whole item.
  #[cfg_attr(feature = "typescript", specta(type = YmapDiffSkip))]
  pub skip: serde_json::Value,
}

/// The content of a file an overwrite was written against.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct OverwriteFingerprint {
  pub sha256: String,
  pub size: u64,
}

impl StreamFileOverwrite {
  /// A digest of this overwrite, for telling whether a merge used the current one.
  pub fn digest(&self) -> String {
    hex::encode(Sha256::digest(serde_json::to_vec(self).unwrap_or_default()))
  }

  /// Leaves this overwrite's skipped differences out of `diff`, the differences of the source
  /// from vanilla as they are now; does nothing when they are not the files it was written
  /// against. `label` names the stream file in the log.
  pub fn apply(
    &self,
    diff: &mut YmapDiffFields,
    source: &OverwriteFingerprint,
    vanilla: &OverwriteFingerprint,
    label: &str,
  ) -> Result<()> {
    if self.source != *source || self.vanilla != *vanilla {
      tracing::warn!(
        "Overwrite of {label} was written against another {}; not applied",
        if self.source != *source { "source file" } else { "vanilla file" }
      );
      return Ok(());
    }
    let unmatched = skip_diffs(diff, &self.skip).map_err(|error| format!("{label}: {error}"))?;
    for path in unmatched {
      tracing::warn!("Overwrite of {label} skips {path}, which is not among the differences");
    }
    Ok(())
  }
}

/// Every resource's overwrites, keyed by resource name, and the latest vanilla files'.
#[derive(Debug, Clone, Default)]
pub struct Overwrites {
  resources: BTreeMap<String, ResourceOverwrite>,
  vanilla: BTreeMap<String, VanillaOverwrite>,
}

impl Overwrites {
  /// Reads `<directory>/resources/*.json` and `<directory>/vanilla/latest/ymap/*.diff.json`; a
  /// missing directory has no overwrites.
  pub fn load(directory: &Path) -> Result<Self> {
    Ok(Self {
      resources: load_resources(directory)?,
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
    self.resources.get(&resource.to_ascii_lowercase())?.files.get(&normalize(stream_path))
  }

  /// Every overwritten stream file as `(resource name, stream path)`.
  pub fn keys(&self) -> impl Iterator<Item = (&str, &str)> {
    self.resources.iter().flat_map(|(resource, overwrite)| {
      overwrite.files.keys().map(move |path| (resource.as_str(), path.as_str()))
    })
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

/// Reads `<directory>/resources/*.json`, keyed by lowercase resource name.
fn load_resources(directory: &Path) -> Result<BTreeMap<String, ResourceOverwrite>> {
  let resources_dir = directory.join("resources");
  let mut resources = BTreeMap::new();
  if !resources_dir.is_dir() {
    return Ok(resources);
  }
  for entry in fs::read_dir(&resources_dir)? {
    let path = entry?.path();
    if !path.is_file() || path.extension().is_none_or(|extension| extension != "json") {
      continue;
    }
    let name = path
      .file_stem()
      .and_then(|name| name.to_str())
      .ok_or_else(|| format!("Overwrite file name is not UTF-8: {}", path.display()))?;
    let overwrite: ResourceOverwrite =
      serde_json::from_reader(BufReader::new(fs::File::open(&path)?))
        .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    if overwrite.schema_version != ResourceOverwrite::SCHEMA_VERSION {
      return Err(
        format!(
          "{} has schema version {}; expected {}",
          path.display(),
          overwrite.schema_version,
          ResourceOverwrite::SCHEMA_VERSION
        )
        .into(),
      );
    }
    let files = overwrite
      .files
      .into_iter()
      .map(|(stream_path, file)| (normalize(&stream_path), file))
      .collect();
    resources.insert(
      name.to_ascii_lowercase(),
      ResourceOverwrite {
        files,
        ..overwrite
      },
    );
  }
  tracing::info!(
    "Loaded overwrites of {} stream files in {} resources from {}",
    resources.values().map(|resource| resource.files.len()).sum::<usize>(),
    resources.len(),
    resources_dir.display()
  );
  Ok(resources)
}

fn normalize(stream_path: &str) -> String {
  stream_path.replace('\\', "/").trim_start_matches('/').to_ascii_lowercase()
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
