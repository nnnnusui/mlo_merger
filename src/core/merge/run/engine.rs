use super::*;

impl MergeYmap {
  pub(super) fn run_inner(
    &self,
    history: Option<&VanillaHistory>,
    latest_files: Option<&[PathBuf]>,
    raw_sources: Option<&[(String, PathBuf)]>,
  ) -> Result<(), Box<dyn std::error::Error>> {
    log::info!(
      "Merging YMAP files from {} and {} into {}",
      self.vanilla_dir.display(),
      self.mod_dir.display(),
      self.output_dir.display()
    );

    // Load blacklist configuration if provided
    let blacklist = if let Some(blacklist_path) = &self.blacklist_config {
      log::info!("Loading blacklist configuration from {}", blacklist_path.display());
      let blacklist_content = fs::read_to_string(blacklist_path)?;
      let config: BlacklistConfig = toml::from_str(&blacklist_content)?;
      log::info!("Loaded {} occlude model blacklist entries", config.occlude_models.len());
      Some(config)
    } else {
      None
    };

    let vanilla = if history.is_none() && latest_files.is_none() {
      Some(VanillaParentCache::update(&self.vanilla_dir)?.0)
    } else {
      None
    };
    let binary_catalog = if let Some(files) = latest_files {
      use crate::core::format::gamefile::{
        meta_resource::{MetaResource, MetaSchemaCatalog},
        resource_file::Rsc7Resource,
      };
      let mut catalog = MetaSchemaCatalog::default();
      for path in files.iter().chain(raw_sources.into_iter().flatten().map(|(_, path)| path)) {
        let bytes = fs::read(path)?;
        if let Ok(resource) = Rsc7Resource::decode(&bytes)
          && let Ok(meta) = MetaResource::parse(&resource)
        {
          catalog.add_resource(&meta);
        }
      }
      Some(catalog)
    } else {
      None
    };
    let mut modded_ymaps_map = if let Some(sources) = raw_sources {
      let mut maps = BTreeMap::<String, Vec<ModYmapReference>>::new();
      for (resource, path) in sources {
        let name = path
          .file_name()
          .and_then(|name| name.to_str())
          .ok_or("Source YMAP filename is not UTF-8")?;
        let reference = ModYmapReference {
          mod_name: resource.clone(),
          ymap_name: name.to_string(),
          mod_ymap_path: path.clone(),
        };
        maps.entry(format!("{name}.xml")).or_default().push(reference);
      }
      maps
    } else {
      collect_modded_ymaps_map(&self.mod_dir)?
    };
    let vanilla_map_paths = latest_files.map(latest_ymap_paths);
    let mut registered_maps = HashSet::new();
    modded_ymaps_map.retain(|_, references| {
      references.retain(|reference| {
        if !reference.mod_ymap_path.is_file() {
          log::warn!(
            "Source YMAP is missing for resource {}; skipping {}",
            reference.mod_name,
            reference.mod_ymap_path.display()
          );
          return false;
        }
        let name = Path::new(&reference.ymap_name).file_stem().and_then(|name| name.to_str());
        let Some(name) = name else {
          log::warn!("Source YMAP has an invalid filename: {}", reference.mod_ymap_path.display());
          return false;
        };
        let hash = reference_hash(name);
        if vanilla_map_paths.as_ref().is_some_and(|paths| !paths.contains_key(&hash)) {
          log::warn!(
            "Latest vanilla YMAP {name}.ymap is missing for resource {}; skipping {}",
            reference.mod_name,
            reference.mod_ymap_path.display()
          );
          return false;
        }
        if !registered_maps.insert((reference.mod_name.clone(), hash)) {
          log::warn!(
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
    log::info!("Found {} unique YMAP files across mods", modded_ymaps_map.len());

    let mut sources = SourceMaps::default();
    let vanilla_count = if let Some(history) = history {
      let files = history.ymap_files();
      for (name, file) in &files {
        sources.register_raw(None, name, history.raw_path(file)?)?;
      }
      files.len()
    } else if let Some(files) = latest_files {
      for path in files {
        let name = path.file_name().ok_or("Vanilla YMAP filename missing")?;
        let name = name.to_str().ok_or("non-UTF8 vanilla filename")?;
        sources.register_raw(None, name, path.clone())?;
      }
      files.len()
    } else {
      let files = vanilla
        .as_ref()
        .expect("legacy merge loads the XML parent cache")
        .paths(&self.vanilla_dir)
        .collect::<Vec<_>>();
      for path in &files {
        let name = path.file_name().unwrap().to_str().ok_or("non-UTF8 vanilla filename")?;
        sources.register(None, name, path.clone())?;
      }
      files.len()
    };
    log::info!("Registered {vanilla_count} vanilla YMAP files for merge");
    for references in modded_ymaps_map.values() {
      for reference in references {
        if raw_sources.is_some() {
          sources.register_raw(
            Some(&reference.mod_name),
            &reference.ymap_name,
            reference.mod_ymap_path.clone(),
          )?;
        } else {
          sources.register(
            Some(&reference.mod_name),
            &reference.ymap_name,
            reference.mod_ymap_path.clone(),
          )?;
        }
      }
    }
    let mut references = ParentReferences::default();
    for (name, versions) in &modded_ymaps_map {
      let hash = reference_hash(name.trim_end_matches(".ymap.xml"));
      if let Some(original) = sources.get(None, hash)? {
        references.register_entities(hash, &original.entities);
      }
      for version in versions {
        if let Some(original) = sources.get(Some(&version.mod_name), hash)? {
          references.register_entities(hash, &original.entities);
        }
      }
    }
    let mut planned = BTreeMap::<u32, PlannedMap>::new();
    let mut duplicate_files = BTreeMap::new();
    for (ymap_name, mod_refs) in &modded_ymaps_map {
      let hash = reference_hash(ymap_name.trim_end_matches(".ymap.xml"));
      let vanilla_ymap_path = vanilla_map_paths
        .as_ref()
        .and_then(|paths| paths.get(&hash))
        .cloned()
        .unwrap_or_else(|| self.vanilla_dir.join(ymap_name));
      if history.is_none() && !self.rebuild_all && mod_refs.len() <= 1 {
        let mod_ref = mod_refs.first().unwrap();
        log::info!("Processing YMAP: {}", vanilla_ymap_path.display());
        log::info!("  Mod: {} ({})", mod_ref.mod_name, mod_ref.mod_ymap_path.display());
        let Some(original) = sources.get(Some(&mod_ref.mod_name), hash)? else {
          log::warn!(
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
      log::info!("Processing YMAP: {}", vanilla_ymap_path.display());

      let Some(original) = sources.get(None, hash)? else {
        log::warn!("Vanilla YMAP is unavailable; skipping {}", vanilla_ymap_path.display());
        continue;
      };
      let (vanilla_ymap, vanilla_entities) =
        sources.normalize(&original, None, hash, &mut references)?;
      if vanilla_ymap.entity_map.len() != vanilla_entities.len() {
        return Err(format!("ambiguous entity identity in {}", vanilla_ymap_path.display()).into());
      }

      let mut ymap_diffs = Vec::new();
      let mut duplicates = Collector::default();
      let vanilla_originals = vanilla_entities
        .iter()
        .zip(&original.entities)
        .map(|(normalized, original)| (normalized.guid, original))
        .collect::<HashMap<_, _>>();
      for mod_info in mod_refs {
        log::info!("  Mod: {} ({})", mod_info.mod_name, mod_info.mod_ymap_path.display());

        let Some(source) = sources.get(Some(&mod_info.mod_name), hash)? else {
          log::warn!(
            "Source YMAP is unavailable for resource {}; skipping {}",
            mod_info.mod_name,
            mod_info.mod_ymap_path.display()
          );
          continue;
        };
        let (mod_ymap, mod_entities) =
          sources.normalize(&source, Some(&mod_info.mod_name), hash, &mut references)?;
        if mod_ymap.entity_map.len() != source.entities.len() {
          return Err(
            format!("ambiguous entity identity in {}", mod_info.mod_ymap_path.display()).into(),
          );
        }
        let ymap_diff = if let Some(history) = history {
          best_cached_ymap_diff(history, &mut sources, &mut references, hash, ymap_name, &mod_ymap)?
        } else {
          YmapDiff::extract_from(&vanilla_ymap, &mod_ymap)
        };
        let source_originals = mod_entities
          .iter()
          .zip(&source.entities)
          .map(|(normalized, original)| (normalized.guid, original))
          .collect::<HashMap<_, _>>();
        for diff in &ymap_diff.entity_diffs {
          let (key, change) = match diff {
            crate::core::format::ymap::diff::YmapEntityDiff::Added(entity) => {
              (entity.guid, Change::Added)
            }
            crate::core::format::ymap::diff::YmapEntityDiff::Removed(entity) => {
              (entity.guid, Change::Removed)
            }
            crate::core::format::ymap::diff::YmapEntityDiff::Modified {
              vanilla,
              ..
            } => (vanilla.guid, Change::Modified),
          };
          duplicates.add(
            diff,
            references.original_guid(key)?,
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

      if ymap_diffs.is_empty() {
        continue;
      }

      let duplicates = duplicates.finish();
      if !duplicates.is_empty() {
        duplicate_files.insert(ymap_name.trim_end_matches(".xml").to_ascii_lowercase(), duplicates);
      }
      let merged_diff = ymap_diffs.into_iter().reduce(|acc, d| acc.merge(d)).unwrap();
      let model = merged_diff.apply_to(&vanilla_ymap, blacklist.as_ref());
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
    let vanilla_children = if let Some(history) = history {
      let mut children = Vec::new();
      for name in history.ymap_names() {
        let hash = reference_hash(name.trim_end_matches(".ymap"));
        let Some(original) = sources.get(None, hash)? else {
          continue;
        };
        if changed_layouts.contains(&reference_hash(&original.model.parent)) {
          children.push((hash, PathBuf::from(name)));
        }
      }
      children
    } else if let Some(files) = latest_files {
      let mut children = Vec::new();
      for path in files {
        let name = path.file_name().ok_or("Vanilla YMAP filename missing")?;
        let name = name.to_str().ok_or("non-UTF8 vanilla filename")?;
        let hash = reference_hash(name.trim_end_matches(".ymap"));
        let Some(original) = sources.get(None, hash)? else {
          continue;
        };
        if changed_layouts.contains(&reference_hash(&original.model.parent)) {
          children.push((hash, PathBuf::from(name)));
        }
      }
      children
    } else {
      vanilla
        .as_ref()
        .expect("legacy merge loads the XML parent cache")
        .children(&self.vanilla_dir, &changed_layouts)
    };
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
          log::warn!(
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
            log::warn!(
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
    for (hash, index, target) in resolved_links {
      if target.map != hash {
        continue;
      }
      let map = &planned[&hash];
      let child = map.entity_at(index).ok_or("final child entity missing")?;
      let ordered = match &map.clone_entities {
        Some(entities) => runtime_entities(entities.iter()),
        None => runtime_entities(map.model.entity_map.values()),
      };
      let parent = ordered.get(child.parent_index as usize).ok_or("final local parent missing")?;
      if !valid_local_pair(child, parent) {
        return Err(
          format!(
            "invalid final local LOD hierarchy for entity {} and parent {}",
            child.guid, parent.guid
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
    log::info!(
      "Repaired parent references; promoted {repaired} clone/vanilla child YMAPs for rebuild"
    );
    self.write_outputs(planned, binary_catalog.as_ref())?;

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
