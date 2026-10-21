use super::*;

impl MergeYmap {
  pub(super) fn run_inner(
    &self,
    latest_files: &[PathBuf],
    raw_sources: &[SourceYmap],
  ) -> Result<(), Box<dyn std::error::Error>> {
    crate::core::config::matching::with_config(|| {
      self.run_configured_inner(latest_files, raw_sources)
    })?
  }

  fn run_configured_inner(
    &self,
    latest_files: &[PathBuf],
    raw_sources: &[SourceYmap],
  ) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!(
      "Merging {} source YMAP files against {} into {}",
      raw_sources.len(),
      self.vanilla_dir.display(),
      self.output_dir.display()
    );

    // Load blacklist configuration if provided
    let blacklist = if let Some(blacklist_path) = &self.blacklist_config {
      tracing::info!("Loading blacklist configuration from {}", blacklist_path.display());
      let blacklist_content = fs::read_to_string(blacklist_path)?;
      let config: BlacklistConfig = toml::from_str(&blacklist_content)?;
      tracing::info!("Loaded {} occlude model blacklist entries", config.occlude_models.len());
      Some(config)
    } else {
      None
    };

    let mut modded_ymaps_map = BTreeMap::<String, Vec<ModYmapReference>>::new();
    for (resource, path, diff_path) in raw_sources {
      let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Source YMAP filename is not UTF-8")?;
      modded_ymaps_map.entry(name.to_string()).or_default().push(ModYmapReference {
        mod_name: resource.clone(),
        ymap_name: name.to_string(),
        mod_ymap_path: path.clone(),
        diff_path: diff_path.clone(),
      });
    }
    let vanilla_map_paths = latest_ymap_paths(latest_files);
    let mut registered_maps = HashSet::new();
    modded_ymaps_map.retain(|_, references| {
      references.retain(|reference| {
        if !reference.mod_ymap_path.is_file() {
          tracing::warn!(
            "Source YMAP is missing for resource {}; skipping {}",
            reference.mod_name,
            reference.mod_ymap_path.display()
          );
          return false;
        }
        let name = Path::new(&reference.ymap_name).file_stem().and_then(|name| name.to_str());
        let Some(name) = name else {
          tracing::warn!(
            "Source YMAP has an invalid filename: {}",
            reference.mod_ymap_path.display()
          );
          return false;
        };
        let hash = reference_hash(name);
        if !vanilla_map_paths.contains_key(&hash) {
          tracing::warn!(
            "Latest vanilla YMAP {name}.ymap is missing for resource {}; skipping {}",
            reference.mod_name,
            reference.mod_ymap_path.display()
          );
          return false;
        }
        if !registered_maps.insert((reference.mod_name.clone(), hash)) {
          tracing::warn!(
            "Duplicate original map {name} within resource {}; keeping the first and skipping {}",
            reference.mod_name,
            reference.mod_ymap_path.display()
          );
          return false;
        }
        true
      });
      !references.is_empty()
    });
    tracing::info!("Found {} unique YMAP files across mods", modded_ymaps_map.len());
    let mut vanilla_overwrites = HashMap::new();
    for (name, overwrite) in &self.vanilla_overwrites {
      let hash = reference_hash(name.trim_end_matches(".ymap"));
      if !vanilla_map_paths.contains_key(&hash) {
        tracing::warn!("Vanilla overwrite of {name} names no latest vanilla YMAP; not applied");
        continue;
      }
      vanilla_overwrites.insert(hash, overwrite);
    }
    let mut overwritten = vanilla_overwrites.keys().copied().collect::<HashSet<_>>();
    for name in modded_ymaps_map.keys() {
      overwritten.remove(&reference_hash(name.trim_end_matches(".ymap")));
    }
    for hash in overwritten {
      let name = vanilla_map_paths[&hash]
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("non-UTF8 vanilla filename")?;
      modded_ymaps_map.entry(name.to_string()).or_default();
    }

    let mut sources = SourceMaps::default();
    for path in latest_files {
      let name = path.file_name().ok_or("Vanilla YMAP filename missing")?;
      let name = name.to_str().ok_or("non-UTF8 vanilla filename")?;
      sources.register(None, name, path.clone())?;
    }
    tracing::info!("Registered {} vanilla YMAP files for merge", latest_files.len());
    for references in modded_ymaps_map.values() {
      for reference in references {
        sources.register(
          Some(&reference.mod_name),
          &reference.ymap_name,
          reference.mod_ymap_path.clone(),
        )?;
      }
    }
    let mut references = ParentReferences::default();
    for (name, versions) in &modded_ymaps_map {
      let hash = reference_hash(name.trim_end_matches(".ymap"));
      if let Some(original) = sources.get(None, hash)? {
        references.register_entities(hash, &original.entities);
      }
      for version in versions {
        if let Some(original) = sources.get(Some(&version.mod_name), hash)? {
          references.register_entities(hash, &original.entities);
        }
      }
    }
    let mut implied = self.implied_diffs(
      &modded_ymaps_map,
      &vanilla_map_paths,
      &mut sources,
      &mut references,
      &registered_maps,
    )?;
    for hash in implied.keys() {
      let merged =
        modded_ymaps_map.keys().any(|name| reference_hash(name.trim_end_matches(".ymap")) == *hash);
      if merged {
        continue;
      }
      let name = vanilla_map_paths[hash]
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("non-UTF8 vanilla filename")?;
      if let Some(original) = sources.get(None, *hash)? {
        references.register_entities(*hash, &original.entities);
      }
      modded_ymaps_map.entry(name.to_string()).or_default();
    }
    let mut planned = BTreeMap::<u32, PlannedMap>::new();
    let mut duplicate_files = BTreeMap::new();
    for (ymap_name, mod_refs) in &modded_ymaps_map {
      let hash = reference_hash(ymap_name.trim_end_matches(".ymap"));
      let vanilla_ymap_path =
        vanilla_map_paths.get(&hash).cloned().unwrap_or_else(|| self.vanilla_dir.join(ymap_name));
      let vanilla_overwrite = vanilla_overwrites.get(&hash).copied();
      // A YMAP of a LOD light pair may take removals from the other, so it is merged.
      if !self.rebuild_all
        && vanilla_overwrite.is_none()
        && LightSide::of(ymap_name).is_none()
        && let [mod_ref] = mod_refs.as_slice()
      {
        tracing::info!("Processing YMAP: {}", vanilla_ymap_path.display());
        tracing::info!("  Mod: {} ({})", mod_ref.mod_name, mod_ref.mod_ymap_path.display());
        let Some(original) = sources.get(Some(&mod_ref.mod_name), hash)? else {
          tracing::warn!(
            "Source YMAP is unavailable for resource {}; skipping {}",
            mod_ref.mod_name,
            mod_ref.mod_ymap_path.display()
          );
          continue;
        };
        let (model, entities) =
          sources.normalize(&original, Some(&mod_ref.mod_name), hash, &mut references)?;
        planned.insert(
          hash,
          PlannedMap {
            name: ymap_name.clone(),
            model,
            clone_entities: Some(entities),
            original,
            copy_target: Some(mod_ref.clone()),
            rebuild: false,
          },
        );
        continue;
      }
      tracing::info!("Processing YMAP: {}", vanilla_ymap_path.display());

      let Some(original) = sources.get(None, hash)? else {
        tracing::warn!("Vanilla YMAP is unavailable; skipping {}", vanilla_ymap_path.display());
        continue;
      };
      // Sources are compared as they are, each entity's parent named by the entity it points
      // at; the merged result is linked into the merge's identities afterwards.
      let vanilla_ymap = &original.model;
      let vanilla_parents = parents_of(&mut sources, &mut references, None, vanilla_ymap)?;

      let mut ymap_diffs = Vec::new();
      let mut duplicates = Collector::default();
      let vanilla_originals =
        original.entities.iter().map(|entity| (entity.guid, entity)).collect::<HashMap<_, _>>();
      for mod_info in mod_refs {
        tracing::info!("  Mod: {} ({})", mod_info.mod_name, mod_info.mod_ymap_path.display());

        let Some(source) = sources.get(Some(&mod_info.mod_name), hash)? else {
          tracing::warn!(
            "Source YMAP is unavailable for resource {}; skipping {}",
            mod_info.mod_name,
            mod_info.mod_ymap_path.display()
          );
          continue;
        };
        references.register_entities(hash, &source.entities);
        let ymap_diff = self.source_diff(
          mod_info,
          &source,
          (&vanilla_ymap_path, vanilla_ymap, &vanilla_parents),
          &mut sources,
          &mut references,
          &registered_maps,
        )?;
        log_diff_counts(&ymap_diff);
        let source_originals =
          source.entities.iter().map(|entity| (entity.guid, entity)).collect::<HashMap<_, _>>();
        for diff in ymap_diff.entities.iter().flatten() {
          let key = diff.guid();
          let change = match diff {
            YmapEntityDiff::Added {
              ..
            } => Change::Added,
            YmapEntityDiff::Removed {
              ..
            } => Change::Removed,
            YmapEntityDiff::Modified {
              ..
            } => Change::Modified,
          };
          duplicates.add(
            diff,
            key,
            vanilla_originals.get(&key).map(|entity| (*entity).clone()),
            Contribution {
              resource: mod_info.mod_name.clone(),
              path: mod_info.mod_ymap_path.clone(),
              change,
              entity: source_originals.get(&key).map(|entity| (*entity).clone()),
            },
          );
        }
        ymap_diffs.push(ymap_diff);
      }
      for (resource, ymap_diff) in implied.remove(&hash).into_iter().flatten() {
        tracing::info!(
          "  LOD light removals {resource} makes only in the other YMAP of the pair (implied)"
        );
        log_diff_counts(&ymap_diff);
        ymap_diffs.push(ymap_diff);
      }
      // Merged as the last source, so the merge policy weighs it against the resources'.
      if let Some(overwrite) = vanilla_overwrite
        && let Some(ymap_diff) =
          overwrite.diffs_for(&file_fingerprint(&vanilla_ymap_path)?, ymap_name)
      {
        tracing::info!("  Vanilla overwrite of {ymap_name}");
        log_diff_counts(ymap_diff);
        let added = ymap_diff
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
        references.register_entities(hash, &added);
        for diff in ymap_diff.entities.iter().flatten() {
          let (change, entity) = match diff {
            YmapEntityDiff::Added {
              entity,
              ..
            } => (Change::Added, Some(entity.clone())),
            YmapEntityDiff::Removed {
              ..
            } => (Change::Removed, None),
            YmapEntityDiff::Modified {
              ..
            } => (Change::Modified, None),
          };
          let key = diff.guid();
          duplicates.add(
            diff,
            key,
            vanilla_originals.get(&key).map(|entity| (*entity).clone()),
            Contribution {
              resource: VANILLA_OVERWRITE_SOURCE.into(),
              path: vanilla_ymap_path.clone(),
              change,
              entity,
            },
          );
        }
        ymap_diffs.push(ymap_diff.clone());
      }

      if ymap_diffs.is_empty() {
        continue;
      }

      let duplicates = duplicates.finish();
      if !duplicates.is_empty() {
        duplicate_files.insert(ymap_name.to_ascii_lowercase(), duplicates);
      }
      let merged_diff = merge_diffs(vanilla_ymap, &ymap_diffs, blacklist.as_ref());
      let failed =
        |error: struct_diff::ApplyError| format!("{}: {error}", vanilla_ymap_path.display());
      let mut model = vanilla_ymap.clone();
      model.apply(&merged_diff).map_err(failed)?;
      let (entities, parents) = entities::apply_linked(
        &vanilla_ymap.entities,
        &vanilla_parents,
        merged_diff.entities.as_deref().unwrap_or_default(),
      )
      .map_err(|error| failed(error.at("entities")))?;
      model.entities =
        link_entities(&mut sources, &mut references, hash, &model, entities, &parents)?;
      planned.insert(
        hash,
        PlannedMap {
          name: ymap_name.clone(),
          model,
          clone_entities: None,
          original,
          copy_target: None,
          rebuild: true,
        },
      );
    }
    let mut changed_layouts = HashSet::new();
    for (hash, output) in &planned {
      if let Some(vanilla) = sources.get(None, *hash)? {
        let original_layout = references.runtime_entity_ids(*hash, &vanilla.entities)?;
        if output.guids() != original_layout {
          changed_layouts.insert(*hash);
        }
      }
    }
    let mut vanilla_children = Vec::new();
    // Only children of a map whose entity order changed need relinking, and planned maps are
    // relinked already, so the other vanilla maps are not read.
    let candidates = if changed_layouts.is_empty() { &[][..] } else { latest_files };
    for path in candidates {
      let name = path.file_name().ok_or("Vanilla YMAP filename missing")?;
      let name = name.to_str().ok_or("non-UTF8 vanilla filename")?;
      let hash = reference_hash(name.trim_end_matches(".ymap"));
      if planned.contains_key(&hash) {
        continue;
      }
      let Some(original) = sources.get(None, hash)? else {
        continue;
      };
      if changed_layouts.contains(&reference_hash(&original.model.parent)) {
        vanilla_children.push((hash, PathBuf::from(name)));
      }
    }
    for (hash, path) in vanilla_children {
      if planned.contains_key(&hash) {
        continue;
      }
      let original = sources.get(None, hash)?.ok_or("vanilla child unavailable")?;
      let (model, entities) = sources.normalize(&original, None, hash, &mut references)?;
      planned.insert(
        hash,
        PlannedMap {
          name: path.file_name().unwrap().to_str().ok_or("non-UTF8 child filename")?.to_string(),
          model,
          clone_entities: Some(entities),
          original,
          copy_target: None,
          rebuild: false,
        },
      );
    }
    let mut layouts =
      planned.iter().map(|(hash, output)| (*hash, output.guids())).collect::<HashMap<_, _>>();
    let mut child_counts = HashMap::<(u32, u32), u32>::new();
    let mut resolved_links = Vec::new();
    let mut repaired = 0usize;
    for (hash, output) in &mut planned {
      let parent_hash = reference_hash(&output.model.parent);
      let map_name = output.name.clone();
      let declared_parent = output.model.parent.clone();
      let entities = output.entities_mut();
      for (child_index, entity) in entities.into_iter().enumerate() {
        let handle = entity.parent_index;
        if handle == -1 {
          continue;
        }
        let owner = references.map_for(handle).ok_or("missing normalized parent handle")?;
        if owner != *hash && owner != parent_hash {
          tracing::warn!(
            "Entity {} in {} references parent YMAP {owner:08X}, but the merged map declares {parent_hash:08X} ({}); detaching the external parent link",
            entity.guid,
            map_name,
            declared_parent
          );
          entity.parent_index = -1;
          entity.flags &= !8;
          if reference_hash(&entity.lod_level) == reference_hash("LODTYPES_DEPTH_HD") {
            entity.lod_level = "LODTYPES_DEPTH_ORPHANHD".into();
          }
          continue;
        }
        if let std::collections::hash_map::Entry::Vacant(entry) = layouts.entry(owner)
          && let Some(vanilla) = sources.get(None, owner)?
        {
          references.register_entities(owner, &vanilla.entities);
          let layout = references.runtime_entity_ids(owner, &vanilla.entities)?;
          entry.insert(layout);
        }
        let index = if let Some(layout) = layouts.get(&owner) {
          references
            .restore(handle, layout)
            .map_err(|error| format!("entity {} parent map {owner:08X}: {error}", entity.guid))?
        } else {
          references
            .unavailable_index(handle)
            .ok_or_else(|| format!("resolved parent map {owner:08X} has no final output"))?
        };
        if let Some(target) = references.target(handle) {
          if index >= 0 {
            *child_counts.entry((target.map, target.guid)).or_default() += 1;
            if owner == *hash {
              entity.flags &= !8;
            } else {
              entity.flags |= 8;
            }
            resolved_links.push((*hash, child_index, target));
          } else {
            tracing::warn!(
              "    Parent {} in map {:08X} removed; detaching entity {}",
              target.guid,
              target.map,
              entity.guid
            );
            entity.flags &= !8;
            if reference_hash(&entity.lod_level) == reference_hash("LODTYPES_DEPTH_HD") {
              entity.lod_level = "LODTYPES_DEPTH_ORPHANHD".into();
            }
          }
        }
        entity.parent_index = index;
      }
    }
    // Each map is assembled once, however many of its entities link within it.
    let mut final_maps = HashMap::<u32, Ymap>::new();
    for (hash, index, target) in resolved_links {
      if target.map != hash {
        continue;
      }
      let map = &planned[&hash];
      let child = map.entity_at(index).ok_or("final child entity missing")?;
      let final_map = final_maps.entry(hash).or_insert_with(|| {
        let mut final_map = map.model.clone();
        if let Some(entities) = &map.clone_entities {
          final_map.entities.clone_from(entities);
        }
        final_map
      });
      if entity_parent_ref(child, final_map, final_map) != EntityParentRef::SelfMap(target.guid) {
        return Err(
          format!(
            "invalid final local LOD hierarchy for entity {} and parent {}",
            child.guid, target.guid
          )
          .into(),
        );
      }
    }
    for (hash, output) in &mut planned {
      if changed_layouts.contains(hash) {
        for entity in output.entities_mut() {
          entity.num_children = child_counts.get(&(*hash, entity.guid)).copied().unwrap_or(0);
        }
      }
      for entity in output.entities_mut() {
        entity.guid = references.original_guid(entity.guid)?;
      }
      if let Some(entities) = &output.clone_entities {
        let changed = entities.iter().zip(&output.original.entities).any(|(after, before)| {
          after.parent_index != before.parent_index
            || after.flags != before.flags
            || after.lod_level != before.lod_level
            || after.num_children != before.num_children
        });
        if changed {
          output.rebuild = true;
          repaired += 1;
        }
      }
      if output.rebuild && output.copy_target.is_none() {
        output.rebuild = output.differs_from_original();
      }
    }
    tracing::info!(
      "Repaired parent references; promoted {repaired} clone/vanilla child YMAPs for rebuild"
    );
    self.write_outputs(planned)?;

    crate::core::vanilla::write_json(
      &self.output_dir.join(".duplicates.json"),
      &DuplicateReport {
        format_version: 1,
        files: duplicate_files,
      },
    )?;
    Ok(())
  }
}

/// The resource a vanilla overwrite's contributions are reported under in duplicate reports.
const VANILLA_OVERWRITE_SOURCE: &str = "overwrite/vanilla/latest";

/// The content of the file at `path` (after resolving links, as latest vanilla files are), as an
/// overwrite records it.
pub(super) fn file_fingerprint(
  path: &Path
) -> Result<OverwriteFingerprint, Box<dyn std::error::Error>> {
  let fingerprint =
    crate::common::fingerprint::Fingerprint::read(&path.canonicalize()?, None, false)?;
  Ok(OverwriteFingerprint {
    sha256: fingerprint.sha256,
    size: fingerprint.size,
  })
}
