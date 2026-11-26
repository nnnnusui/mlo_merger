//! How many polygons each loaded vanilla YBN repeats, for merges to build the YBNs where
//! repeats are common without them; `ybn_repeats.json` in the vanilla cache.

use std::collections::BTreeMap;

use rayon::prelude::*;

use super::generations::SourceYmap;
use super::*;
use crate::core::format::ybn::{diff::polygons, read_ybn};

/// File name of the repeat index in the vanilla cache.
pub(crate) const YBN_REPEATS_FILE: &str = "ybn_repeats.json";
/// Bump when what counts as a repeat, or the layout, changes.
pub(crate) const ALGORITHM_VERSION: u32 = 1;

/// The loaded vanilla YBNs holding repeats: polygons the very same as an earlier one of the
/// same collision flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YbnRepeatIndex {
  pub format_version: u32,
  pub algorithm_version: u32,
  pub vanilla_manifest_sha256: String,
  pub version: String,
  /// Keyed by lowercase file name.
  pub files: BTreeMap<String, YbnRepeatCount>,
}

/// The repeats of one YBN.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YbnRepeatCount {
  pub sha256: String,
  /// The polygons it holds that can be compared.
  pub polygons: usize,
  /// How many of them repeat an earlier one.
  pub repeats: usize,
}

impl YbnRepeatIndex {
  /// Reads the index of the vanilla cache at `cache_dir`, `None` when it has none of this
  /// algorithm version.
  pub(crate) fn load(cache_dir: &Path) -> Result<Option<Self>> {
    let path = cache_dir.join(YBN_REPEATS_FILE);
    if !fs::is_file(&path) {
      return Ok(None);
    }
    let index: Self = crate::common::json::read_json(&path)
      .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    Ok((index.algorithm_version == ALGORITHM_VERSION).then_some(index))
  }
}

/// Counts the repeats of the YBNs of `ybns` the game loads (those not `inactive`) and writes the
/// index into `staging`.
pub(super) fn write_repeats(
  staging: &Path,
  ybns: &BTreeMap<String, SourceYmap>,
  inactive: &BTreeMap<String, String>,
  revision: &str,
  version: &str,
) -> Result<YbnRepeatIndex> {
  let counts = ybns
    .par_iter()
    .filter(|(name, _)| !inactive.contains_key(*name))
    .filter_map(|(name, source)| {
      let counted = fs::read(&source.path)
        .map_err(|error| error.to_string())
        .and_then(|bytes| read_ybn(&bytes).map_err(|error| error.to_string()))
        .and_then(|root| polygons::repeats(&root).map_err(|error| error.to_string()));
      match counted {
        Ok((polygons, repeats)) => (repeats > 0).then(|| {
          (
            name.clone(),
            YbnRepeatCount {
              sha256: source.sha256.clone(),
              polygons,
              repeats,
            },
          )
        }),
        // A YBN whose polygons cannot be compared, such as one with a primitive root, has none
        // to leave out.
        Err(error) => {
          tracing::debug!("Not counting the repeats of {name}: {error}");
          None
        }
      }
    })
    .collect::<BTreeMap<_, _>>();
  let index = YbnRepeatIndex {
    format_version: CACHE_SCHEMA,
    algorithm_version: ALGORITHM_VERSION,
    vanilla_manifest_sha256: revision.to_owned(),
    version: version.to_owned(),
    files: counts,
  };
  write_json(&staging.join(YBN_REPEATS_FILE), &index)?;
  Ok(index)
}

/// Whether the cache at `output_dir` has the repeat index of `revision` at `version`.
pub(super) fn index_is_current(
  output_dir: &Path,
  revision: &str,
  version: &str,
) -> bool {
  YbnRepeatIndex::load(output_dir).ok().flatten().is_some_and(|index| {
    index.format_version == CACHE_SCHEMA
      && index.vanilla_manifest_sha256 == revision
      && index.version == version
  })
}
