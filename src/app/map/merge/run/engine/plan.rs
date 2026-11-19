//! Planning each YMAP: merged from every diff of it, or kept from its single source.

use super::*;
use crate::app::map::merge::duplicates::{IgnoredReason, Target};
use struct_diff::vec::ItemDiff;

/// The resource a vanilla overwrite's contributions are reported under in duplicate reports.
const VANILLA_OVERWRITE_SOURCE: &str = "overwrite/vanilla";

impl<'a> MergeRun<'a> {
  /// Plans the YMAP named `ymap_name`: kept from its single source as it is when nothing else
  /// changes it, otherwise merged from every diff of it.
  pub(super) fn plan_ymap(
    &mut self,
    ymap_name: &str,
    mod_refs: &[ModYmapReference],
    vanilla_overwrite: Option<&VanillaOverwrite>,
    implied: &mut ImpliedDiffs,
    propagated: &mut PropagatedDiffs,
  ) -> Result<(), Box<dyn std::error::Error>> {
    let hash = reference_hash(ymap_name.trim_end_matches(".ymap"));
    let vanilla_ymap_path = self
      .vanilla_map_paths
      .get(&hash)
      .cloned()
      .unwrap_or_else(|| self.merge.vanilla_dir.join(ymap_name));
    tracing::info!("Processing YMAP: {}", vanilla_ymap_path.display());
    // A YMAP of a LOD light pair may take removals from the other, so it is merged, as is one
    // taking changes carried over from an older generation.
    if !self.merge.rebuild_all
      && vanilla_overwrite.is_none()
      && LightSide::of(ymap_name).is_none()
      && !propagated.contains_key(&hash)
      && let [mod_ref] = mod_refs
    {
      return self.plan_single(ymap_name, hash, mod_ref);
    }
    let Some(original) = self.sources.get(None, hash)? else {
      tracing::warn!("Vanilla YMAP is unavailable; skipping {}", vanilla_ymap_path.display());
      return Ok(());
    };
    // Sources are compared as they are, each entity's parent named by the entity it points
    // at; the merged result is linked into the merge's identities afterwards.
    let vanilla_ymap = &original.model;
    let vanilla_parents = parents_of(&mut self.sources, &mut self.references, None, vanilla_ymap)?;
    let added = self.merge.added.contains(&ymap_name.to_ascii_lowercase());
    // The archive holds a vanilla YMAP at `{version}/ymap/{file}`.
    let mut duplicates = Collector::default();
    duplicates.loaded_version = vanilla_ymap_path
      .strip_prefix(&self.merge.vanilla_dir)
      .ok()
      .filter(|path| path.components().count() > 1)
      .and_then(|path| path.components().next())
      .map(|version| version.as_os_str().to_string_lossy().into_owned());
    let mut ymap_diffs = Vec::new();
    // The resource, file and origin generation of each of `ymap_diffs` recorded.
    let mut contributors = HashMap::<usize, Contributor>::new();
    for mod_info in mod_refs {
      tracing::info!("  Mod: {} ({})", mod_info.mod_name, mod_info.mod_ymap_path.display());
      let Some(source) = self.sources.get(Some(&mod_info.mod_name), hash)? else {
        tracing::warn!(
          "Source YMAP is unavailable for resource {}; skipping {}",
          mod_info.mod_name,
          mod_info.mod_ymap_path.display()
        );
        continue;
      };
      self.references.register_entities(hash, &source.entities);
      let mut ymap_diff = self.merge.source_diff(
        mod_info,
        &source,
        (&vanilla_ymap_path, vanilla_ymap, &vanilla_parents),
        &mut self.sources,
        &mut self.references,
        &self.registered_maps,
      )?;
      log_diff_counts(&ymap_diff);
      // An added YMAP's baseline is empty, so its name, flags, extents and block are its own.
      if !added {
        leave_out(&mut ymap_diff, IgnoredChange::MERGE);
      }
      contributors.insert(
        ymap_diffs.len(),
        (mod_info.mod_name.clone(), mod_info.mod_ymap_path.clone(), None),
      );
      ymap_diffs.push(ymap_diff);
    }
    for (resource, mut ymap_diff) in implied.remove(&hash).into_iter().flatten() {
      tracing::info!(
        "  LOD light removals {resource} makes only in the other YMAP of the pair (implied)"
      );
      log_diff_counts(&ymap_diff);
      leave_out(&mut ymap_diff, IgnoredChange::MERGE);
      ymap_diffs.push(ymap_diff);
    }
    for (resource, path, mut ymap_diff) in propagated.remove(&hash).into_iter().flatten() {
      tracing::info!(
        "  Changes {resource} makes to the same items of an older generation (propagated)"
      );
      log_diff_counts(&ymap_diff);
      leave_out(&mut ymap_diff, IgnoredChange::MERGE);
      // A carried document lies at `{resource}/{version}/{file}` below the merge's generations.
      let origin = path
        .parent()
        .and_then(Path::file_name)
        .map(|version| version.to_string_lossy().into_owned());
      contributors.insert(ymap_diffs.len(), (resource, path, origin));
      ymap_diffs.push(ymap_diff);
    }
    // Merged as the last source, so the merge policy weighs it against the resources'. The
    // changes left out of the resources' diffs are kept, as they apply only from here, except
    // the values the merge derives.
    if let Some(overwrite) = vanilla_overwrite
      && let Some(ymap_diff) =
        overwrite.diffs_for(&OverwriteFingerprint::read(&vanilla_ymap_path)?, ymap_name)
    {
      tracing::info!("  Vanilla overwrite of {ymap_name}");
      log_diff_counts(ymap_diff);
      let mut ymap_diff = ymap_diff.clone();
      leave_out(&mut ymap_diff, IgnoredChange::DERIVED);
      let ymap_diff = &ymap_diff;
      let added_entities = ymap_diff
        .entities
        .iter()
        .flatten()
        .filter_map(|diff| match diff {
          YmapEntityDiff::Added {
            entity,
            ..
          } => Some(entity.clone()),
          _ => None,
        })
        .collect::<Vec<_>>();
      self.references.register_entities(hash, &added_entities);
      contributors.insert(
        ymap_diffs.len(),
        (VANILLA_OVERWRITE_SOURCE.to_owned(), vanilla_ymap_path.clone(), None),
      );
      ymap_diffs.push(ymap_diff.clone());
    }
    if ymap_diffs.is_empty() {
      return Ok(());
    }
    // Sources adding the same entity, as a resource's patch shipping its YMAP again does, add it
    // once: the additions an earlier source makes already are left out. The versions of an added
    // YMAP are thinned below, as they are wholly additions.
    if !added && ymap_diffs.len() > 1 {
      let before = ymap_diffs.clone();
      let thinned =
        compare_added::thin_additions(&mut ymap_diffs, &crate::core::config::matching::current());
      record_thinned(&mut duplicates, &before, &thinned.left_out, &contributors);
      if !thinned.left_out.is_empty() {
        tracing::info!(
          "  Left out {} additions an earlier source makes already",
          thinned.left_out.len()
        );
      }
      for (left_out, kept) in thinned.aliases {
        self.references.alias(hash, left_out, kept)?;
      }
    }
    for (index, diff) in ymap_diffs.iter().enumerate() {
      if let Some(contributor) = contributors.get(&index) {
        record_entity_changes(&mut duplicates, diff, &original.entities, contributor);
        record_item_changes(&mut duplicates, diff, contributor);
      }
    }
    // Several resources' versions of a YMAP they add are all additions to the empty YMAP:
    // those an earlier version makes already are left out, and the extents cover them all.
    if added && ymap_diffs.len() > 1 {
      let tolerances = crate::core::config::matching::current();
      let thinned = compare_added::thin_additions(&mut ymap_diffs, &tolerances);
      if !thinned.left_out.is_empty() {
        tracing::info!(
          "  Left out {} additions an earlier version makes already",
          thinned.left_out.len()
        );
      }
      // Children of an entity left out point at the one kept in its place.
      for (left_out, kept) in thinned.aliases {
        self.references.alias(hash, left_out, kept)?;
      }
      compare_added::union_extents(&mut ymap_diffs);
    }
    // What the versions of an added YMAP share is recorded by the overwrite cache, with the
    // additions placed alike that GUIDs do not pair.
    let duplicates = duplicates.finish();
    if !duplicates.is_empty() && !added {
      self.duplicate_files.insert(ymap_name.to_ascii_lowercase(), duplicates);
    }
    let merged_diff = merge_diffs(vanilla_ymap, &ymap_diffs);
    let failed =
      |error: struct_diff::ApplyError| format!("{}: {error}", vanilla_ymap_path.display());
    // An added YMAP no other resource ships is compared with its source, which is deployed as
    // it is unless the merge changes it.
    let compared = match mod_refs {
      [single] if added && !self.merge.replacing.contains(&ymap_name.to_ascii_lowercase()) => {
        self.sources.get(Some(&single.mod_name), hash)?.unwrap_or_else(|| Rc::clone(&original))
      }
      _ => Rc::clone(&original),
    };
    let mut model = vanilla_ymap.clone();
    model.apply(&merged_diff).map_err(failed)?;
    let (entities, parents) = entities::apply_linked(
      &vanilla_ymap.entities,
      &vanilla_parents,
      merged_diff.entities.as_deref().unwrap_or_default(),
    )
    .map_err(|error| failed(error.at("entities")))?;
    model.entities =
      link_entities(&mut self.sources, &mut self.references, hash, &model, entities, &parents)?;
    self.planned.insert(
      hash,
      PlannedMap {
        name: ymap_name.to_string(),
        model,
        clone_entities: None,
        original: compared,
        copy_target: None,
        rebuild: true,
      },
    );
    Ok(())
  }

  /// Plans the YMAP named `ymap_name` (name hash `hash`) as `mod_ref`, its single source, ships
  /// it, with its entities linked into the merge's identities.
  pub(super) fn plan_single(
    &mut self,
    ymap_name: &str,
    hash: u32,
    mod_ref: &ModYmapReference,
  ) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("  Mod: {} ({})", mod_ref.mod_name, mod_ref.mod_ymap_path.display());
    let Some(original) = self.sources.get(Some(&mod_ref.mod_name), hash)? else {
      tracing::warn!(
        "Source YMAP is unavailable for resource {}; skipping {}",
        mod_ref.mod_name,
        mod_ref.mod_ymap_path.display()
      );
      return Ok(());
    };
    let (model, entities) =
      self.sources.normalize(&original, Some(&mod_ref.mod_name), hash, &mut self.references)?;
    self.planned.insert(
      hash,
      PlannedMap {
        name: ymap_name.to_string(),
        model,
        clone_entities: Some(entities),
        original,
        copy_target: Some(mod_ref.clone()),
        rebuild: false,
      },
    );
    Ok(())
  }
}

/// The resource, file and origin generation of a source's diff; the origin is that of a change
/// carried from another generation.
type Contributor = (String, PathBuf, Option<String>);

/// Records in `duplicates` each addition of `left_out`, which [`compare_added::thin_additions`]
/// left out of `diffs` (as they were before) for the same addition of an earlier diff: as
/// [`IgnoredReason::SameAdditionNewer`] when the same resource makes that one in a newer
/// generation, otherwise as an identical duplicate when the two are equal and, for an entity,
/// for the earlier source's when they differ; `contributors` names each diff's source.
fn record_thinned(
  duplicates: &mut Collector,
  diffs: &[YmapDiffFields],
  left_out: &[compare_added::LeftOut],
  contributors: &HashMap<usize, Contributor>,
) {
  let added_entity =
    |(diff, change): (usize, usize)| match diffs[diff].entities.as_ref()?.get(change)? {
      YmapEntityDiff::Added {
        entity,
        target_index,
        ..
      } => Some((entity, *target_index)),
      _ => None,
    };
  let loaded_version = duplicates.loaded_version.clone();
  let contribution = |(resource, path, origin): &Contributor, index| Contribution {
    resource: resource.clone(),
    path: path.clone(),
    change: Change::Added,
    index,
    origin: origin.clone(),
  };
  for left_out in left_out {
    let (Some(kept), Some(source)) =
      (contributors.get(&left_out.kept.0), contributors.get(&left_out.at.0))
    else {
      continue;
    };
    let (target, identical, (index, kept_index)) = if left_out.field == "entities" {
      let (Some((entity, index)), Some((kept_entity, kept_index))) =
        (added_entity(left_out.at), added_entity(left_out.kept))
      else {
        continue;
      };
      (
        Target::Entity {
          guid: entity.guid,
        },
        entity == kept_entity,
        (index, kept_index),
      )
    } else {
      let (Some(index), Some(kept_index)) = (
        added_item(&diffs[left_out.at.0], left_out.field, left_out.at.1),
        added_item(&diffs[left_out.kept.0], left_out.field, left_out.kept.1),
      ) else {
        continue;
      };
      // Other items are left out only when they equal the kept one within the tolerances.
      let target = Target::Item {
        array: left_out.field.to_owned(),
        base_index: None,
      };
      (target, true, (index, kept_index))
    };
    let (kept_contribution, left_contribution) =
      (contribution(kept, kept_index), contribution(source, index));
    let newer = crate::app::map::merge::duplicates::newer(
      &kept_contribution,
      &left_contribution,
      loaded_version.as_deref(),
    );
    let reason = if kept.0 == source.0 && newer {
      IgnoredReason::SameAdditionNewer
    } else if identical {
      IgnoredReason::IdenticalDuplicate
    } else {
      IgnoredReason::EarlierSourceWins
    };
    duplicates.add_ignored(target, kept_contribution, left_contribution, reason);
  }
}

/// The `target_index` of addition `change` of the array `field` of `diff`, as
/// [`compare_added::LeftOut`] names arrays; `None` when it is no addition.
fn added_item(
  diff: &YmapDiffFields,
  field: &str,
  change: usize,
) -> Option<Option<usize>> {
  fn added<T, C>(
    list: Option<&Vec<ItemDiff<T, C>>>,
    change: usize,
  ) -> Option<Option<usize>> {
    match list?.get(change)? {
      ItemDiff::Added {
        target_index,
        ..
      } => Some(*target_index),
      _ => None,
    }
  }
  match field {
    "box_occluders" => added(diff.box_occluders.as_ref(), change),
    "occlude_models" => added(diff.occlude_models.as_ref(), change),
    "time_cycle_modifiers" => added(diff.time_cycle_modifiers.as_ref(), change),
    "car_generators" => added(diff.car_generators.as_ref(), change),
    "instanced_data.grass_instance_list" => {
      added(diff.instanced_data.as_ref().and_then(|data| data.grass_instance_list.as_ref()), change)
    }
    _ => None,
  }
}

/// Records in `duplicates` each removal, modification or replacement of a vanilla item of the
/// arrays of `diff` other than the entities, which `contributor` makes; arrays are named by
/// their path in a diff.
fn record_item_changes(
  duplicates: &mut Collector,
  diff: &YmapDiffFields,
  contributor: &Contributor,
) {
  record_items(duplicates, "box_occluders", diff.box_occluders.as_ref(), contributor);
  record_items(duplicates, "occlude_models", diff.occlude_models.as_ref(), contributor);
  record_items(duplicates, "lod_lights", diff.lod_lights.as_ref(), contributor);
  record_items(
    duplicates,
    "distant_lod_lights.items",
    diff.distant_lod_lights.as_ref().and_then(|lights| lights.items.as_ref()),
    contributor,
  );
  record_items(duplicates, "time_cycle_modifiers", diff.time_cycle_modifiers.as_ref(), contributor);
  record_items(duplicates, "car_generators", diff.car_generators.as_ref(), contributor);
  record_items(
    duplicates,
    "instanced_data.grass_instance_list",
    diff.instanced_data.as_ref().and_then(|data| data.grass_instance_list.as_ref()),
    contributor,
  );
}

/// [`record_item_changes`] for the array `array`, with the changes `changes`.
fn record_items<T: serde::Serialize, C: serde::Serialize>(
  duplicates: &mut Collector,
  array: &str,
  changes: Option<&Vec<ItemDiff<T, C>>>,
  (resource, path, origin): &Contributor,
) {
  // Indices differ between sources, so identical changes are compared without them.
  fn json(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap_or_default()
  }
  for change in changes.into_iter().flatten() {
    let (kind, base_index, signature) = match change {
      ItemDiff::Added {
        ..
      } => continue,
      ItemDiff::Removed {
        base_index,
        ..
      } => (Change::Removed, *base_index, "Removed".to_owned()),
      ItemDiff::Modified {
        base_index,
        changes,
        ..
      } => (Change::Modified, *base_index, format!("Modified {}", json(changes))),
      ItemDiff::Replaced {
        base_index,
        item,
        ..
      } => (Change::Replaced, *base_index, format!("Replaced {}", json(item))),
    };
    duplicates.add_item(
      array,
      base_index,
      signature,
      Contribution {
        resource: resource.clone(),
        path: path.clone(),
        change: kind,
        index: Some(base_index),
        origin: origin.clone(),
      },
    );
  }
}

/// Records in `duplicates` each entity change of `diff`, which `contributor` (its resource, file
/// and origin generation) makes, with the GUID of the entity it adds or of the entity of
/// `vanilla` it changes.
fn record_entity_changes(
  duplicates: &mut Collector,
  diff: &YmapDiffFields,
  vanilla: &[YmapEntity],
  (resource, path, origin): &Contributor,
) {
  for change in diff.entities.iter().flatten() {
    let guid = change.guid(vanilla);
    let (kind, index) = match change {
      YmapEntityDiff::Added {
        target_index,
        ..
      } => (Change::Added, *target_index),
      YmapEntityDiff::Removed {
        base_index,
        ..
      } => (Change::Removed, Some(*base_index)),
      YmapEntityDiff::Modified {
        base_index,
        ..
      } => (Change::Modified, Some(*base_index)),
    };
    duplicates.add(
      change,
      guid,
      Contribution {
        resource: resource.clone(),
        path: path.clone(),
        change: kind,
        index,
        origin: origin.clone(),
      },
    );
  }
}
