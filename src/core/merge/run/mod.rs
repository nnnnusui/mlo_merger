use super::duplicates::{Change, Collector, Contribution, DuplicateReport};
use crate::core::config::blacklist::BlacklistConfig;
use crate::core::format::ymap::compare::{
  EntityParent, EntityParents, YmapComparisonDiff, YmapDiffContext, YmapDiffFields, YmapEntityDiff,
  entities, entity_parents, merge_diffs,
};
use crate::core::format::ymap::model::YmapEntity;
use crate::core::format::ymap::model::ymap::Ymap;
use crate::core::format::ymap::reference_hash;
use crate::core::format::ymap::relation::{EntityParentRef, entity_parent_ref};
use crate::core::merge::ymap_parent_refs::{
  OriginalMap, ParentReferences, SourceMaps, runtime_entities,
};
use crate::core::overwrite::{OverwriteFingerprint, StreamFileOverwrite, VanillaOverwrite};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use struct_diff::Diff;

/// A source YMAP to merge: its resource, path and source-cache diff document, if any.
pub type SourceYmap = (String, PathBuf, Option<PathBuf>);

#[derive(Debug, Clone)]
pub struct MergeYmap {
  pub vanilla_dir: PathBuf,
  pub output_dir: PathBuf,
  pub rebuild_all: bool,
  pub blacklist_config: Option<PathBuf>,
  /// The overwrites of the sources, keyed by source path, applied to their diffs.
  pub overwrites: HashMap<PathBuf, StreamFileOverwrite>,
  /// The overwrites of the latest vanilla files, keyed by lowercase file name, merged after the
  /// sources' diffs. A vanilla file with one is merged even when no source replaces it.
  pub vanilla_overwrites: HashMap<String, VanillaOverwrite>,
}

impl MergeYmap {
  /// Merges source maps against the supplied latest vanilla files and emits edited RSC7 binaries.
  ///
  /// Sources given with a source-cache diff document are merged through it, which must have been
  /// taken from the source and vanilla files as they are now; the others are compared here.
  pub fn run_with_latest_vanilla_files(
    &self,
    vanilla_files: &[PathBuf],
    source_files: &[SourceYmap],
  ) -> Result<(), Box<dyn std::error::Error>> {
    self.run_inner(vanilla_files, source_files)
  }
}

mod engine;
mod inputs;
mod output;
mod plan;

use inputs::{ModYmapReference, latest_ymap_paths};
use plan::PlannedMap;

#[cfg(test)]
mod tests;

/// Logs how many items of each array a source changes.
fn log_diff_counts(diff: &YmapDiffFields) {
  let instanced = diff.instanced_data.as_ref();
  let counts = [
    ("entities", diff.entities.as_ref().map_or(0, Vec::len)),
    ("box_occluders", diff.box_occluders.as_ref().map_or(0, Vec::len)),
    ("occlude_models", diff.occlude_models.as_ref().map_or(0, Vec::len)),
    ("lod_lights", diff.lod_lights.as_ref().map_or(0, Vec::len)),
    (
      "distant_lod_lights",
      diff.distant_lod_lights.as_ref().and_then(|it| it.items.as_ref()).map_or(0, Vec::len),
    ),
    ("physics_dictionaries", diff.physics_dictionaries.as_ref().map_or(0, Vec::len)),
    ("prop_instances", instanced.and_then(|it| it.prop_instance_list.as_ref()).map_or(0, Vec::len)),
    ("grass_batches", instanced.and_then(|it| it.grass_instance_list.as_ref()).map_or(0, Vec::len)),
    ("time_cycle_modifiers", diff.time_cycle_modifiers.as_ref().map_or(0, Vec::len)),
    ("car_generators", diff.car_generators.as_ref().map_or(0, Vec::len)),
  ];
  let changed = counts
    .iter()
    .filter(|(_, count)| *count > 0)
    .map(|(name, count)| format!("{name}={count}"))
    .collect::<Vec<_>>();
  tracing::info!(
    "    Changed items: {}",
    if changed.is_empty() { "none".into() } else { changed.join(", ") }
  );
}

/// The parent of each entity of `ymap`, read from `resource`'s or else vanilla's parent YMAP.
fn parents_of(
  sources: &mut SourceMaps,
  references: &mut ParentReferences,
  resource: Option<&str>,
  ymap: &Ymap,
) -> Result<EntityParents, Box<dyn std::error::Error>> {
  let parent_hash = reference_hash(&ymap.parent);
  let parent = if parent_hash == 0 { None } else { sources.get(resource, parent_hash)? };
  if let Some(parent) = &parent {
    references.register_entities(parent_hash, &parent.entities);
  }
  Ok(entity_parents(ymap, parent.as_ref().map(|parent| parent.entities.as_slice())))
}

/// `entities` of the merged `model` (map `hash`) with the merge's internal identities: GUIDs
/// replaced by entity ids and parents by the handles the final phase restores to indices.
fn link_entities(
  sources: &mut SourceMaps,
  references: &mut ParentReferences,
  hash: u32,
  model: &Ymap,
  mut entities: Vec<YmapEntity>,
  parents: &[EntityParent],
) -> Result<Vec<YmapEntity>, Box<dyn std::error::Error>> {
  let ids = references.entity_ids(hash, &entities)?;
  for (entity, (id, parent)) in entities.iter_mut().zip(ids.into_iter().zip(parents)) {
    let original_guid = entity.guid;
    entity.guid = id;
    let (owner, guid) = match parent {
      EntityParent::None => {
        entity.parent_index = -1;
        continue;
      }
      EntityParent::Unresolved {
        in_parent,
        index,
      } => {
        let owner = if *in_parent { reference_hash(&model.parent) } else { hash };
        entity.parent_index = unresolved(sources, references, owner, *index)?;
        continue;
      }
      EntityParent::Own {
        guid,
      } => (hash, *guid),
      EntityParent::Parent {
        map,
        guid,
      } => (*map, *guid),
    };
    entity.parent_index = match references.capture_guid(owner, guid) {
      Ok(handle) => handle,
      Err(error) => {
        tracing::warn!(
          "    Unresolved parent for entity {original_guid}: {error}; changes to its parent layout will be rejected"
        );
        let index = vanilla_index(sources, owner, guid)?.unwrap_or(-1);
        unresolved(sources, references, owner, index)?
      }
    };
  }
  Ok(entities)
}

/// A handle keeping `index` into map `owner` as it is, valid while the vanilla layout of `owner`
/// is unchanged, or while `owner` stays unavailable.
fn unresolved(
  sources: &mut SourceMaps,
  references: &mut ParentReferences,
  owner: u32,
  index: i32,
) -> Result<i32, Box<dyn std::error::Error>> {
  let layout = match sources.get(None, owner)? {
    Some(vanilla) => Some(references.runtime_entity_ids(owner, &vanilla.entities)?),
    None => None,
  };
  Ok(references.capture_unresolved(None, owner, layout.as_deref(), index)?)
}

/// The runtime index of the first entity with `guid` in the vanilla map `owner`.
fn vanilla_index(
  sources: &mut SourceMaps,
  owner: u32,
  guid: u32,
) -> Result<Option<i32>, Box<dyn std::error::Error>> {
  let Some(vanilla) = sources.get(None, owner)? else {
    return Ok(None);
  };
  Ok(
    runtime_entities(vanilla.entities.iter())
      .iter()
      .position(|entity| entity.guid == guid)
      .and_then(|index| i32::try_from(index).ok()),
  )
}
