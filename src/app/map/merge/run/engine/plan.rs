//! Planning each YMAP: merged from every diff of it, or kept from its single source.

use super::*;

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
    let mut duplicates = Collector::default();
    let mut ymap_diffs = Vec::new();
    // Each resource's diffs among `ymap_diffs`, its own and those carried from other generations.
    let mut by_resource = HashMap::<String, Vec<usize>>::new();
    // The resource, file and origin generation of each of `ymap_diffs` recorded.
    let mut contributors = HashMap::<usize, (String, PathBuf, Option<String>)>::new();
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
      let contributor = (mod_info.mod_name.clone(), mod_info.mod_ymap_path.clone(), None);
      record_entity_changes(&mut duplicates, &ymap_diff, &original.entities, &contributor);
      by_resource.entry(mod_info.mod_name.clone()).or_default().push(ymap_diffs.len());
      contributors.insert(ymap_diffs.len(), contributor);
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
      let contributor = (resource.clone(), path.clone(), origin);
      // What the resource adds here already, itself or from a newer generation, is added once.
      let earlier = by_resource.entry(resource.clone()).or_default().clone();
      if !earlier.is_empty() {
        let before = earlier
          .iter()
          .map(|&index| ymap_diffs[index].clone())
          .chain([ymap_diff])
          .collect::<Vec<_>>();
        let mut diffs = before.clone();
        let thinned =
          compare_added::thin_additions(&mut diffs, &crate::core::config::matching::current());
        ymap_diff = diffs.pop().ok_or("Thinned diffs lost the carried one")?;
        let carried = before.len() - 1;
        let added =
          |(diff, change): (usize, usize)| match before[diff].entities.as_ref()?.get(change)? {
            YmapEntityDiff::Added {
              entity,
              target_index,
              ..
            } => Some((entity.guid, *target_index)),
            _ => None,
          };
        for left_out in thinned
          .left_out
          .iter()
          .filter(|left_out| left_out.field == "entities" && left_out.at.0 == carried)
        {
          let (Some((guid, index)), Some((_, kept_index))) =
            (added(left_out.at), added(left_out.kept))
          else {
            continue;
          };
          let Some(kept) = contributors.get(&earlier[left_out.kept.0]) else {
            continue;
          };
          let contribution = |(resource, path, origin): &(String, PathBuf, Option<String>),
                              index| Contribution {
            resource: resource.clone(),
            path: path.clone(),
            change: Change::Added,
            index,
            origin: origin.clone(),
          };
          duplicates.add_thinned(
            guid,
            contribution(kept, kept_index),
            contribution(&contributor, index),
          );
        }
        if !thinned.left_out.is_empty() {
          tracing::info!(
            "    Left out {} additions {resource} makes to a newer generation too",
            thinned.left_out.len()
          );
        }
        for (left_out, kept) in thinned.aliases {
          self.references.alias(hash, left_out, kept)?;
        }
      }
      record_entity_changes(&mut duplicates, &ymap_diff, &original.entities, &contributor);
      by_resource.entry(resource).or_default().push(ymap_diffs.len());
      contributors.insert(ymap_diffs.len(), contributor);
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
      record_entity_changes(
        &mut duplicates,
        ymap_diff,
        &original.entities,
        &(VANILLA_OVERWRITE_SOURCE.to_owned(), vanilla_ymap_path.clone(), None),
      );
      ymap_diffs.push(ymap_diff.clone());
    }
    if ymap_diffs.is_empty() {
      return Ok(());
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

/// Records in `duplicates` each entity change of `diff`, which `contributor` (its resource, file
/// and origin generation) makes, with the GUID of the entity it adds or of the entity of
/// `vanilla` it changes.
fn record_entity_changes(
  duplicates: &mut Collector,
  diff: &YmapDiffFields,
  vanilla: &[YmapEntity],
  (resource, path, origin): &(String, PathBuf, Option<String>),
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
