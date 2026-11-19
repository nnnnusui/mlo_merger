//! Writes what keeps each resource's LOD light pairs in step, as
//! `implied/{resource}/{stream path}.json` beside the diff documents, which stay as they are.
//!
//! The game reads the `lod_lights` of a `lodlights_*` YMAP and the `distant_lod_lights` of its
//! `distlodlights_*` partner as parallel arrays (see
//! [`lod_light_pairs`](crate::core::format::ymap::compare::lod_light_pairs)). Of a resource's YMAP
//! of a pair whose vanilla YMAPs hold as many lights each:
//!
//! - Lights it adds that the resource's other YMAP of the pair, in the same directory, does not
//!   add at the same index, or all of them when the resource does not ship it, are to be left
//!   out of its diff, as a `skip` in the shape of an overwrite's.
//! - When the resource does not ship the other YMAP, the lights it removes are to be removed
//!   from the vanilla other one too, as a diff of that one (`partner`), marked `implied`. A
//!   resource shipping both has those removals in its diff documents already.
//!
//! The documents are derived from the diff documents and the latest vanilla files, so
//! `implied/inputs.json` records a digest of the diff record and the vanilla cache revision, and
//! the documents are kept while it matches.

use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::Result;
use crate::{
  app::map::source_cache::{read_metadata, stream_file_diff_path, types::SourceFile},
  common::{json::write_json, path::ensure_safe_path},
  core::format::ymap::compare::{
    ItemDiff, YmapComparisonDiff, YmapComparisonFile, YmapDiffFields, lod_light_pairs::LightSide,
  },
  platform::fs,
};

/// The directory of the documents in the source cache.
pub(in crate::app::map::source_cache) const IMPLIED_DIR: &str = "implied";
/// File name of the record of what the documents were derived from, in [`IMPLIED_DIR`].
const INPUTS_FILE: &str = "inputs.json";
/// Bump when [`ImpliedRecord`] or how documents are derived changes.
const RECORD_VERSION: u32 = 1;

/// What keeps one of a resource's YMAPs of a LOD light pair in step with the other; see the
/// module docs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapImplied {
  pub schema_version: u32,
  /// The resource, by its ID.
  pub resource: String,
  /// The source YMAP and its baseline, as its diff document records them.
  pub target: YmapComparisonFile,
  pub base: YmapComparisonFile,
  /// The additions to leave out of the diff document, in the shape of an overwrite's `skip`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional, type = Option<crate::app::map::overwrite::YmapDiffSkip>))]
  pub skip: Option<Value>,
  /// The removals of the vanilla other YMAP of the pair, which the resource does not ship.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub partner: Option<YmapImpliedPartner>,
}

impl YmapImplied {
  /// The current [`Self::schema_version`].
  pub const SCHEMA_VERSION: u32 = 1;
}

/// Removals of a vanilla YMAP a resource's other YMAP of its LOD light pair implies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapImpliedPartner {
  /// The vanilla YMAP, in lowercase.
  pub file_name: String,
  /// The latest vanilla YMAP, as it was when the removals were taken.
  pub base: YmapComparisonFile,
  pub diffs: YmapDiffFields,
}

/// What the documents in [`IMPLIED_DIR`] were derived from, and the documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ImpliedRecord {
  record_version: u32,
  digest: String,
  /// Each document, relative to [`IMPLIED_DIR`].
  documents: Vec<String>,
}

/// Where the source cache keeps the implied document of a resource's YMAP, relative to its
/// `implied` directory, given the resource's source directory and the file's path below it.
pub(crate) fn implied_document_path(
  resource_source: &Path,
  file_path: &str,
) -> Result<PathBuf> {
  let diff = stream_file_diff_path(resource_source, file_path)?;
  let name = diff.to_string_lossy();
  Ok(PathBuf::from(format!("{}.json", name.trim_end_matches(".diff.json"))))
}

/// The documents [`write_implied`] keeps, relative to the source cache; empty when it has no
/// current record.
pub(in crate::app::map::source_cache) fn recorded_documents(output: &Path) -> Vec<String> {
  crate::common::json::read_json::<ImpliedRecord>(output.join(IMPLIED_DIR).join(INPUTS_FILE))
    .ok()
    .filter(|record| record.record_version == RECORD_VERSION)
    .map(|record| {
      record.documents.into_iter().map(|path| format!("{IMPLIED_DIR}/{path}")).collect()
    })
    .unwrap_or_default()
}

/// Rebuilds [`IMPLIED_DIR`] from the diff documents in `output` (the source cache) and the
/// latest vanilla files of the vanilla cache at `vanilla_cache_dir`, unless its record shows it
/// current; returns how many documents it holds.
pub(in crate::app::map::source_cache) fn write_implied(
  output: &Path,
  vanilla_cache_dir: &Path,
) -> Result<usize> {
  let target = output.join(IMPLIED_DIR);
  let staging = output.join(format!("{IMPLIED_DIR}.tmp"));
  ensure_safe_path(output, &target)?;
  ensure_safe_path(output, &staging)?;
  let metadata = read_metadata(output)?;
  let digest = hex::encode(Sha256::digest(serde_json::to_vec(&(
    RECORD_VERSION,
    YmapImplied::SCHEMA_VERSION,
    hex::encode(Sha256::digest(fs::read(output.join("diff").join("inputs.json"))?)),
    &metadata.vanilla_cache_revision,
  ))?));
  if let Ok(record) = crate::common::json::read_json::<ImpliedRecord>(target.join(INPUTS_FILE))
    && record.record_version == RECORD_VERSION
    && record.digest == digest
    && record.documents.iter().all(|path| fs::is_file(target.join(path)))
  {
    return Ok(record.documents.len());
  }

  remove_dir_if_exists(&staging)?;
  fs::create_dir_all(&staging)?;
  let vanilla = crate::app::map::vanilla_cache::ymap_bases(vanilla_cache_dir)?
    .into_iter()
    .map(|file| (file.name.to_ascii_lowercase(), (file.path, file.object)))
    .collect::<BTreeMap<_, _>>();
  let mut pairs = Pairs {
    vanilla: &vanilla,
    counts: BTreeMap::new(),
  };
  let mut documents = Vec::new();
  for (resource_id, resource) in &metadata.resources {
    let ymaps = resource.files_by_format.get(".ymap").map(Vec::as_slice).unwrap_or_default();
    let diff = |file: &SourceFile| -> Result<Option<YmapComparisonDiff>> {
      let path = output.join("diff").join(stream_file_diff_path(&resource.source, &file.path)?);
      Ok(crate::common::json::read_json::<YmapComparisonDiff>(&path).ok())
    };
    for file in ymaps {
      let name = file.file_name.to_ascii_lowercase();
      let (Some(side), Some(partner_name)) = (LightSide::of(&name), LightSide::partner_name(&name))
      else {
        continue;
      };
      if !pairs.in_step(&name, &partner_name, side)? {
        continue;
      }
      let Some(document) = diff(file)? else {
        continue;
      };
      let directory = |path: &str| path.rsplit_once('/').map(|(directory, _)| directory.to_owned());
      let partner = ymaps.iter().find(|other| {
        other.file_name.eq_ignore_ascii_case(&partner_name)
          && directory(&other.path) == directory(&file.path)
      });
      let partner_added = match partner {
        Some(partner) => {
          diff(partner)?.map(|diff| added(&diff.diffs, side.other())).unwrap_or_default()
        }
        None => BTreeSet::new(),
      };
      let unpaired =
        added(&document.diffs, side).difference(&partner_added).copied().collect::<BTreeSet<_>>();
      let implied_partner = match partner {
        Some(_) => None,
        None => {
          let removed = side.removed(&document.diffs);
          let mut diffs = YmapDiffFields::default();
          side.other().imply_removals(&mut diffs, &removed);
          (!removed.is_empty())
            .then(|| -> Result<YmapImpliedPartner> {
              Ok(YmapImpliedPartner {
                base: pairs.vanilla_file(&partner_name)?,
                file_name: partner_name.clone(),
                diffs,
              })
            })
            .transpose()?
        }
      };
      if unpaired.is_empty() && implied_partner.is_none() {
        continue;
      }
      let implied = YmapImplied {
        schema_version: YmapImplied::SCHEMA_VERSION,
        resource: resource_id.clone(),
        target: document.target.clone(),
        base: document.base.clone(),
        skip: (!unpaired.is_empty()).then(|| skip_additions(side, &unpaired)),
        partner: implied_partner,
      };
      let relative = implied_document_path(&resource.source, &file.path)?;
      let path = staging.join(&relative);
      ensure_safe_path(&staging, &path)?;
      fs::create_dir_all(path.parent().ok_or("Implied document path has no parent")?)?;
      write_json(&path, &implied)?;
      documents.push(relative.to_string_lossy().replace('\\', "/"));
    }
  }
  write_json(
    &staging.join(INPUTS_FILE),
    &ImpliedRecord {
      record_version: RECORD_VERSION,
      digest,
      documents: documents.clone(),
    },
  )?;
  remove_dir_if_exists(&target)?;
  fs::rename(&staging, &target)?;
  tracing::info!("Wrote {} LOD light pair documents in {}", documents.len(), target.display());
  Ok(documents.len())
}

/// The latest vanilla LOD light pairs, read as they are needed.
struct Pairs<'a> {
  /// The latest vanilla files keyed by lowercase file name, with their paths in the vanilla
  /// archive.
  vanilla: &'a BTreeMap<String, (PathBuf, String)>,
  /// How many lights of each side each vanilla YMAP read so far holds.
  counts: BTreeMap<String, Option<usize>>,
}

impl Pairs<'_> {
  /// Whether the vanilla YMAPs `name`, holding `side`, and `partner` hold as many lights each.
  fn in_step(
    &mut self,
    name: &str,
    partner: &str,
    side: LightSide,
  ) -> Result<bool> {
    let own = self.count(name, side)?;
    let other = self.count(partner, side.other())?;
    Ok(own.is_some() && own == other)
  }

  fn count(
    &mut self,
    name: &str,
    side: LightSide,
  ) -> Result<Option<usize>> {
    if let Some(count) = self.counts.get(name) {
      return Ok(*count);
    }
    let count = match self.vanilla.get(name) {
      Some((path, _)) => Some(side.count(&crate::app::map::vanilla::read_ymap(path)?)),
      None => None,
    };
    self.counts.insert(name.to_owned(), count);
    Ok(count)
  }

  /// The latest vanilla YMAP `name` as documents record it.
  fn vanilla_file(
    &self,
    name: &str,
  ) -> Result<YmapComparisonFile> {
    let (path, object) =
      self.vanilla.get(name).ok_or_else(|| format!("No latest vanilla YMAP named {name}"))?;
    let fingerprint = crate::common::fingerprint::Fingerprint::read(path, None, false)?;
    Ok(YmapComparisonFile {
      path: object.clone(),
      sha256: fingerprint.sha256,
      size: fingerprint.size,
      modified_seconds: fingerprint.modified_seconds,
      modified_nanos: fingerprint.modified_nanos,
    })
  }
}

/// The target indices of the lights of `side` that `diffs` adds.
fn added(
  diffs: &YmapDiffFields,
  side: LightSide,
) -> BTreeSet<usize> {
  fn targets<T, C>(items: Option<&[ItemDiff<T, C>]>) -> BTreeSet<usize> {
    items
      .into_iter()
      .flatten()
      .filter_map(|item| match item {
        ItemDiff::Added {
          target_index,
          ..
        } => *target_index,
        _ => None,
      })
      .collect()
  }
  match side {
    LightSide::LodLights => targets(diffs.lod_lights.as_deref()),
    LightSide::DistantLodLights => {
      targets(diffs.distant_lod_lights.as_ref().and_then(|lights| lights.items.as_deref()))
    }
  }
}

/// A skip leaving out the additions of the lights of `side` at `targets`.
fn skip_additions(
  side: LightSide,
  targets: &BTreeSet<usize>,
) -> Value {
  let selectors = targets
    .iter()
    .map(|target| serde_json::json!({ "Added": { "target_index": target } }))
    .collect::<Vec<_>>();
  match side {
    LightSide::LodLights => serde_json::json!({ "lod_lights": selectors }),
    LightSide::DistantLodLights => {
      serde_json::json!({ "distant_lod_lights": { "items": selectors } })
    }
  }
}

fn remove_dir_if_exists(path: &Path) -> Result<()> {
  match fs::remove_dir_all(path) {
    Ok(()) => Ok(()),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(error) => Err(error.into()),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn skips_name_the_additions_of_their_side() {
    let targets = BTreeSet::from([3, 7]);
    assert_eq!(
      skip_additions(LightSide::DistantLodLights, &targets),
      serde_json::json!({ "distant_lod_lights": { "items": [
        { "Added": { "target_index": 3 } }, { "Added": { "target_index": 7 } },
      ] } })
    );
    assert_eq!(
      skip_additions(LightSide::LodLights, &BTreeSet::from([1])),
      serde_json::json!({ "lod_lights": [{ "Added": { "target_index": 1 } }] })
    );
  }

  #[test]
  fn additions_are_read_by_target_index() {
    let diffs: YmapDiffFields = serde_json::from_value(serde_json::json!({
      "distant_lod_lights": { "items": [
        { "Added": { "anchor_base_index": 4, "target_index": 5,
          "item": { "position": "1 2 3", "rgbi": "2164235622" } } },
        { "Removed": { "base_index": 2 } },
      ] },
      "lod_lights": [
        { "Added": { "anchor_base_index": 4, "target_index": 9, "item": {
          "direction": { "x": 0.0, "y": 0.0, "z": -1.0 }, "falloff": 2.0, "falloff_exponent": 64.0,
          "time_and_state_flags": "149946431", "hash": "2011493", "cone_inner_angle": 7,
          "cone_outer_angle_or_cap_ext": 127, "corona_intensity": 6.0,
        } } },
      ],
    }))
    .unwrap();
    assert_eq!(added(&diffs, LightSide::DistantLodLights), BTreeSet::from([5]));
    assert_eq!(added(&diffs, LightSide::LodLights), BTreeSet::from([9]));
  }
}
