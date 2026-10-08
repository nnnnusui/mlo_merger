use super::cache_inputs::VanillaHistory;
use super::duplicates::{Change, Collector, Contribution, DuplicateReport};
use crate::core::common::function::collect_files_with_suffix;
use crate::core::config::blacklist::BlacklistConfig;
use crate::core::diff_cache::ymap_distance;
use crate::core::extract::ExtractYmap;
use crate::core::format::ymap::diff::{YmapDiff, reference_hash};
use crate::core::format::ymap::model::ymap::Ymap;
use crate::core::format::ymap::xml::XmlYmap;
use crate::core::merge::{
  ymap_parent_cache::VanillaParentCache,
  ymap_parent_refs::{
    OriginalMap, ParentReferences, SourceMaps, patch_clone, runtime_entities, valid_local_pair,
  },
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct MergeYmap {
  pub vanilla_dir: PathBuf,
  pub mod_dir: PathBuf,
  pub mod_ymap_dir: PathBuf,
  pub output_dir: PathBuf,
  pub rebuild_all: bool,
  pub blacklist_config: Option<PathBuf>,
}

impl MergeYmap {
  pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
    self.run_inner(None, None, None)
  }

  /// Version-aware YMAP merge API for the planned historical-baseline workflow.
  pub fn run_with_vanilla_cache(
    &self,
    history: &VanillaHistory,
  ) -> Result<(), Box<dyn std::error::Error>> {
    self.run_inner(Some(history), None, None)
  }

  /// Merges source maps against the supplied latest vanilla files and emits edited RSC7 binaries.
  pub fn run_with_latest_vanilla_files(
    &self,
    vanilla_files: &[PathBuf],
    source_files: Option<&[(String, PathBuf)]>,
  ) -> Result<(), Box<dyn std::error::Error>> {
    self.run_inner(None, Some(vanilla_files), source_files)
  }

  fn run_inner(
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
    let copy_targets_txt = self.output_dir.join("_copy_targets.txt");
    fs::create_dir_all(copy_targets_txt.parent().unwrap())?;
    let clone_ymap_dir = self.output_dir.join("clone");
    fs::create_dir_all(&clone_ymap_dir)?;
    let mut copy_targets_file = fs::File::create(copy_targets_txt)?;
    let managed = planned
      .values()
      .map(|output| output.name.trim_end_matches(".xml"))
      .collect::<Vec<_>>()
      .join("\n");
    fs::write(self.output_dir.join("_managed_ymaps.txt"), managed)?;
    for output in planned.into_values() {
      let xml_path = self.output_dir.join(&output.name);
      let binary_name = output.name.trim_end_matches(".xml");
      let binary_path = self.output_dir.join(binary_name);
      let clone_path = clone_ymap_dir.join(binary_name);
      if output.rebuild {
        if let Some(catalog) = &binary_catalog {
          let entities = match &output.clone_entities {
            Some(entities) => runtime_entities(entities.iter()),
            None => runtime_entities(output.model.entity_map.values()),
          }
          .into_iter()
          .cloned()
          .collect::<Vec<_>>();
          let bytes =
            crate::core::format::ymap::binary::write_ymap(&output.model, &entities, catalog)
              .map_err(|error| {
                std::io::Error::new(error.kind(), format!("Cannot write {binary_name}: {error}"))
              })?;
          fs::write(&binary_path, bytes)?;
          if xml_path != binary_path && xml_path.exists() {
            fs::remove_file(&xml_path)?;
          }
          if clone_path.exists() {
            fs::remove_file(&clone_path)?;
          }
          log::info!("  [Success] Wrote merged/relinked YMAP binary to: {}", binary_path.display());
          continue;
        }
        let xml_string = if let Some(entities) = &output.clone_entities {
          if output.original.xml.is_empty() {
            let mut model = output.model.clone();
            model.entity_map =
              entities.iter().map(|entity| (entity.guid, entity.clone())).collect();
            let xml_ymap: XmlYmap = model.into();
            let mut xml = String::new();
            let mut serializer = quick_xml::se::Serializer::new(&mut xml);
            serializer.indent(' ', 2);
            xml_ymap.serialize(serializer)?;
            xml
          } else {
            patch_clone(&output.original, entities)?
          }
        } else {
          let xml_ymap: XmlYmap = output.model.into();
          let mut xml = String::new();
          let mut serializer = quick_xml::se::Serializer::new(&mut xml);
          serializer.indent(' ', 2);
          xml_ymap.serialize(serializer)?;
          xml
        };
        fs::write(&xml_path, xml_string)?;
        if clone_path.exists() {
          fs::remove_file(&clone_path)?;
        }
        log::info!("  [Success] Wrote merged/relinked YMAP to: {}", xml_path.display());
      } else if let Some(target) = output.copy_target {
        let ymap_xml_name = target.mod_ymap_path.file_name().unwrap().to_string_lossy();
        let extracted_ymap_name = ymap_xml_name.trim_end_matches(".xml");
        use std::io::Write;
        writeln!(copy_targets_file, "{}", ymap_xml_name)?;
        fs::copy(self.mod_ymap_dir.join(extracted_ymap_name), &clone_path)?;
        if xml_path.exists() {
          fs::remove_file(&xml_path)?;
        }
        log::info!(
          "Copied single-mod YMAP to clone output (vanilla diff merge skipped): {binary_name}"
        );
      } else {
        if binary_catalog.is_some() && binary_path.exists() {
          fs::remove_file(&binary_path)?;
        }
        if xml_path.exists() {
          fs::remove_file(&xml_path)?;
        }
        if clone_path.exists() {
          fs::remove_file(&clone_path)?;
        }
      }
    }

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

fn latest_ymap_paths(files: &[PathBuf]) -> HashMap<u32, PathBuf> {
  let mut paths = HashMap::new();
  for path in files {
    if let Some(name) = path.file_stem().and_then(|name| name.to_str()) {
      paths.entry(reference_hash(name)).or_insert_with(|| path.clone());
    }
  }
  paths
}

fn best_cached_ymap_diff(
  history: &VanillaHistory,
  sources: &mut SourceMaps,
  references: &mut ParentReferences,
  hash: u32,
  name: &str,
  modded: &Ymap,
) -> Result<YmapDiff, Box<dyn std::error::Error>> {
  let mut candidates = Vec::new();
  for (version, file) in history.candidate_files(name)? {
    let original = OriginalMap::load_raw(&history.raw_path(&file)?)?;
    let (candidate, entities) = sources.normalize(&original, None, hash, references)?;
    if candidate.entity_map.len() != entities.len() {
      continue;
    }
    candidates.push((version, candidate));
  }
  let (score, version, diff) = best_ymap_diff(candidates, modded)?;
  log::info!("Best YMAP baseline {name}: {version} ({score} changes)");
  Ok(diff)
}

fn best_ymap_diff(
  candidates: Vec<(String, Ymap)>,
  modded: &Ymap,
) -> Result<(usize, String, YmapDiff), Box<dyn std::error::Error>> {
  let mut best: Option<(usize, String, YmapDiff)> = None;
  for (version, candidate) in candidates {
    let score = ymap_distance(&candidate, modded)?;
    log::info!("Candidate YMAP baseline {version}: {score} changes");
    if best.as_ref().is_none_or(|(best_score, _, _)| score < *best_score) {
      best = Some((score, version, YmapDiff::extract_from(&candidate, modded)));
    }
  }
  best.ok_or_else(|| "No usable vanilla YMAP candidates".into())
}

struct PlannedMap {
  name: String,
  model: Ymap,
  clone_entities: Option<Vec<crate::core::format::ymap::model::YmapEntity>>,
  original: Rc<OriginalMap>,
  copy_target: Option<ModYmapReference>,
  rebuild: bool,
}

impl PlannedMap {
  /// Compares runtime entity order and final fields without internal normalized entity keys.
  fn differs_from_original(&self) -> bool {
    let mut before = self.original.model.clone();
    let mut after = self.model.clone();
    before.entity_map.clear();
    after.entity_map.clear();
    if before != after {
      return true;
    }
    let entities = match &self.clone_entities {
      Some(entities) => runtime_entities(entities.iter()),
      None => runtime_entities(self.model.entity_map.values()),
    };
    entities != runtime_entities(self.original.entities.iter())
  }

  fn entity_at(
    &self,
    index: usize,
  ) -> Option<&crate::core::format::ymap::model::YmapEntity> {
    match &self.clone_entities {
      Some(entities) => entities.get(index),
      None => self.model.entity_map.get_index(index).map(|(_, entity)| entity),
    }
  }

  fn guids(&self) -> Vec<u32> {
    let entities = match &self.clone_entities {
      Some(entities) => runtime_entities(entities.iter()),
      None => runtime_entities(self.model.entity_map.values()),
    };
    entities.into_iter().map(|entity| entity.guid).collect()
  }

  fn entities_mut(&mut self) -> Vec<&mut crate::core::format::ymap::model::YmapEntity> {
    match &mut self.clone_entities {
      Some(entities) => entities.iter_mut().collect(),
      None => self.model.entity_map.values_mut().collect(),
    }
  }
}

/// Information about a modded ymap file
#[derive(Debug, Clone)]
struct ModYmapReference {
  mod_name: String,
  ymap_name: String,
  mod_ymap_path: PathBuf,
}

fn collect_modded_ymaps_map(
  mod_dir: &Path
) -> Result<BTreeMap<String, Vec<ModYmapReference>>, Box<dyn std::error::Error>> {
  let mod_files = collect_files_with_suffix(mod_dir, ".ymap.xml");
  let mut map: BTreeMap<String, Vec<ModYmapReference>> = BTreeMap::new();

  for mod_file in mod_files {
    // Extract file name
    let file_name = mod_file.file_name().and_then(|n| n.to_str()).ok_or("Invalid file name")?;

    // Extract mod name and ymap name from "modname___ymapname.ymap.xml" format
    if let Some((mod_name, ymap_xml_name)) = file_name.split_once(ExtractYmap::FLATTEN_DELIMITER) {
      let ymap_name = ymap_xml_name.trim_end_matches(".xml");
      let info = ModYmapReference {
        mod_name: mod_name.to_string(),
        ymap_name: ymap_name.to_string(),
        mod_ymap_path: mod_file.clone(),
      };

      map.entry(ymap_xml_name.to_string()).or_default().push(info);
    } else {
      log::warn!(
        "Warning: Invalid file name format (expected 'mod___ymap.ymap.xml'): {}",
        file_name
      );
    }
  }

  Ok(map)
}

#[cfg(test)]
fn parse_ymap_xml(file_path: &Path) -> Result<Ymap, Box<dyn std::error::Error>> {
  let xml_content = fs::read_to_string(file_path)?;
  let xml_ymap: XmlYmap = quick_xml::de::from_str(&xml_content)?;
  if let Some(error) = &xml_ymap.instanced_data.error {
    return Err(
      std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("{} contains an instancedData error: {error}", file_path.display()),
      )
      .into(),
    );
  }
  Ok(xml_ymap.into())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::{
    format::ymap::diff::{BatchKey, reference_hash},
    format::ymap::model::{GrassInstance, GrassInstanceBatch},
  };
  use std::collections::{BTreeSet, HashMap, HashSet};

  #[test]
  fn cached_ymap_diff_uses_best_baseline_and_preserves_latest_changes() {
    let xml: XmlYmap = quick_xml::de::from_str(include_str!(concat!(
      env!("CARGO_MANIFEST_DIR"),
      "/docs/sample/parent_refs/vanilla_parent.ymap.xml"
    )))
    .unwrap();
    let baseline: Ymap = xml.into();
    let entity_id = *baseline.entity_map.keys().next().unwrap();
    let mut modded = baseline.clone();
    modded.entity_map.get_mut(&entity_id).unwrap().position.x += 1.0;
    let mut latest = baseline.clone();
    latest.flags ^= 1;

    let (score, version, diff) =
      best_ymap_diff(vec![("latest".into(), latest.clone()), ("base".into(), baseline)], &modded)
        .unwrap();

    assert_eq!(version, "base");
    assert_eq!(score, 1);
    let merged = diff.apply_to(&latest, None);
    assert_eq!(merged.entity_map[&entity_id].position.x, modded.entity_map[&entity_id].position.x);
    assert_eq!(merged.flags, latest.flags);
  }

  #[test]
  #[ignore = "requires local vanilla YMAP schemas"]
  fn latest_ymap_merge_writes_binary_without_xml() {
    use crate::core::format::gamefile::{
      meta_resource::{MetaResource, MetaSchemaCatalog},
      meta_xml::ymap_to_model_with_entities,
      resource_file::Rsc7Resource,
    };
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap");
    let mut files =
      fs::read_dir(base).unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
    files.sort();
    let root = std::env::temp_dir().join(format!("ymap_direct_merge_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("vanilla")).unwrap();
    fs::create_dir_all(root.join("source")).unwrap();
    let source = files
      .iter()
      .find(|file| file.file_stem().unwrap().to_string_lossy().contains("occl"))
      .unwrap();
    let bytes = fs::read(source).unwrap();
    let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
    let mut compatible = MetaSchemaCatalog::default();
    compatible.add_resource(&meta);
    let (model, entities) = ymap_to_model_with_entities(&bytes, &compatible.hash_names).unwrap();
    let name = source.file_name().unwrap();
    let vanilla = root.join("vanilla").join(name);
    let source_path = root.join("source").join(name);
    fs::write(&vanilla, &bytes).unwrap();
    fs::write(&source_path, &bytes).unwrap();
    let merge = MergeYmap {
      vanilla_dir: root.join("vanilla"),
      mod_dir: root.join("source"),
      mod_ymap_dir: root.join("source"),
      output_dir: root.join("output"),
      rebuild_all: true,
      blacklist_config: None,
    };
    merge
      .run_with_latest_vanilla_files(
        std::slice::from_ref(&vanilla),
        Some(&[("resource".into(), source_path.clone())]),
      )
      .unwrap();
    assert!(!root.join("output").join(name).exists());
    let mut edited = model.clone();
    let bit = (!model.content_flags).trailing_zeros();
    assert!(bit < 32);
    edited.content_flags |= 1u32 << bit;
    fs::write(
      &source_path,
      crate::core::format::ymap::binary::write_ymap(&edited, &entities, &compatible).unwrap(),
    )
    .unwrap();
    merge
      .run_with_latest_vanilla_files(
        std::slice::from_ref(&vanilla),
        Some(&[("resource".into(), source_path.clone())]),
      )
      .unwrap();
    let output = fs::read(root.join("output").join(name)).unwrap();
    assert!(output.starts_with(b"RSC7"));
    assert!(!root.join("output").join(format!("{}.xml", name.to_string_lossy())).exists());
    let (restored, restored_entities) =
      ymap_to_model_with_entities(&output, &compatible.hash_names).unwrap();
    assert_eq!(restored.content_flags, edited.content_flags);
    assert_eq!(restored_entities, entities);
    fs::write(&source_path, bytes).unwrap();
    merge
      .run_with_latest_vanilla_files(&[vanilla], Some(&[("resource".into(), source_path)]))
      .unwrap();
    assert!(!root.join("output").join(name).exists());
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn latest_ymap_log_paths_use_actual_native_files() {
    let path = PathBuf::from("vanilla-cache/latest/ymap/vw_lodlights_small037.ymap");
    let paths = latest_ymap_paths(std::slice::from_ref(&path));
    let internal_name = "vw_lodlights_small037.ymap.xml";
    let hash = reference_hash(internal_name.trim_end_matches(".ymap.xml"));
    assert_eq!(paths[&hash], path);
    assert_eq!(paths[&hash].extension().unwrap(), "ymap");
    let duplicate = PathBuf::from("another-cache/vw_lodlights_small037.ymap");
    assert_eq!(latest_ymap_paths(&[path.clone(), duplicate])[&hash], path);
  }

  #[test]
  #[ignore = "requires local vanilla YMAP schemas"]
  fn native_ymap_merge_deletion_wins_over_retained_entities_in_both_orders() {
    use crate::core::format::gamefile::{
      meta_resource::{MetaResource, MetaSchemaCatalog},
      meta_xml::ymap_to_model_with_entities,
      resource_file::Rsc7Resource,
    };
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("asset/vanilla-cache/latest/ymap");
    let mut files =
      fs::read_dir(base).unwrap().map(|entry| entry.unwrap().path()).collect::<Vec<_>>();
    files.sort();
    let sample = files
      .iter()
      .find(|path| {
        ymap_to_model_with_entities(&fs::read(path).unwrap(), &HashMap::new()).is_ok_and(
          |(model, entities)| !entities.is_empty() && model.entity_map.len() == entities.len(),
        )
      })
      .unwrap();
    let bytes = fs::read(sample).unwrap();
    let meta = MetaResource::parse(&Rsc7Resource::decode(&bytes).unwrap()).unwrap();
    let mut catalog = MetaSchemaCatalog::default();
    catalog.add_resource(&meta);
    let (model, entities) = ymap_to_model_with_entities(&bytes, &catalog.hash_names).unwrap();
    let guid = entities[0].guid;
    let remaining =
      entities.iter().filter(|entity| entity.guid != guid).cloned().collect::<Vec<_>>();
    let root = std::env::temp_dir().join(format!("ymap_delete_native_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for directory in ["vanilla", "deleted", "retained"] {
      fs::create_dir_all(root.join(directory)).unwrap();
    }
    let name = sample.file_name().unwrap();
    let vanilla = root.join("vanilla").join(name);
    let deleted = root.join("deleted").join(name);
    let retained = root.join("retained").join(name);
    fs::write(&vanilla, &bytes).unwrap();
    fs::write(&retained, &bytes).unwrap();
    fs::write(
      &deleted,
      crate::core::format::ymap::binary::write_ymap(&model, &remaining, &catalog).unwrap(),
    )
    .unwrap();
    let merge = MergeYmap {
      vanilla_dir: root.join("vanilla"),
      mod_dir: root.clone(),
      mod_ymap_dir: root.clone(),
      output_dir: root.join("output"),
      rebuild_all: true,
      blacklist_config: None,
    };
    for reverse in [false, true] {
      let mut sources =
        vec![("deleted".into(), deleted.clone()), ("retained".into(), retained.clone())];
      if reverse {
        sources.reverse();
      }
      merge.run_with_latest_vanilla_files(std::slice::from_ref(&vanilla), Some(&sources)).unwrap();
      let bytes = fs::read(root.join("output").join(name)).unwrap();
      let (_, result) = ymap_to_model_with_entities(&bytes, &catalog.hash_names).unwrap();
      assert!(!result.iter().any(|entity| entity.guid == guid));
      assert_eq!(result.len(), remaining.len());
    }
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn final_ymap_edit_detection_ignores_internal_keys_but_keeps_repairs() {
    let sample =
      Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/parent_refs/vanilla_parent.ymap.xml");
    let original = Rc::new(OriginalMap::load(&sample).unwrap());
    let mut output = PlannedMap {
      name: "parent.ymap.xml".into(),
      model: original.model.clone(),
      clone_entities: None,
      original: Rc::clone(&original),
      copy_target: None,
      rebuild: true,
    };
    output.model.entity_map = original
      .entities
      .iter()
      .enumerate()
      .map(|(index, entity)| (index as u32 + 1, entity.clone()))
      .collect();
    assert!(!output.differs_from_original());
    output.model.flags ^= 1;
    assert!(output.differs_from_original());
    output.model.flags ^= 1;
    output.model.entity_map.swap_indices(0, 1);
    assert!(output.differs_from_original());
    output.model.entity_map.swap_indices(0, 1);
    assert!(!output.differs_from_original());
    output.clone_entities = Some(original.entities.clone());
    assert!(!output.differs_from_original());
    output.clone_entities.as_mut().unwrap()[0].parent_index += 1;
    assert!(output.differs_from_original());
    output.clone_entities = Some(original.entities.clone());
    output.clone_entities.as_mut().unwrap()[0].flags ^= 8;
    assert!(output.differs_from_original());
  }

  #[test]
  fn merge_emits_only_edited_ymaps_when_rebuilding() {
    let root = std::env::temp_dir().join(format!("ymap_edited_only_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let vanilla = root.join("vanilla");
    let mods = root.join("mods");
    let output = root.join("output");
    fs::create_dir_all(&vanilla).unwrap();
    fs::create_dir_all(&mods).unwrap();
    let sample =
      Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/parent_refs/vanilla_parent.ymap.xml");
    fs::copy(&sample, vanilla.join("parent.ymap.xml")).unwrap();
    fs::copy(&sample, mods.join("resource_a___parent.ymap.xml")).unwrap();
    let merge = MergeYmap {
      vanilla_dir: vanilla,
      mod_dir: mods.clone(),
      mod_ymap_dir: mods.clone(),
      output_dir: output.clone(),
      rebuild_all: true,
      blacklist_config: None,
    };
    merge.run().unwrap();
    assert!(!output.join("parent.ymap.xml").exists());
    fs::copy(&sample, mods.join("resource_b___parent.ymap.xml")).unwrap();
    merge.run().unwrap();
    assert!(!output.join("parent.ymap.xml").exists());
    let mut edited = parse_ymap_xml(&sample).unwrap();
    edited.entity_map.values_mut().next().unwrap().position.x += 1.0;
    let xml: XmlYmap = edited.into();
    fs::write(mods.join("resource_a___parent.ymap.xml"), quick_xml::se::to_string(&xml).unwrap())
      .unwrap();
    merge.run().unwrap();
    assert!(output.join("parent.ymap.xml").exists());
    fs::write(
      mods.join("resource_b___parent.ymap.xml"),
      fs::read(mods.join("resource_a___parent.ymap.xml")).unwrap(),
    )
    .unwrap();
    merge.run().unwrap();
    let duplicates: DuplicateReport = serde_json::from_reader(std::io::BufReader::new(
      fs::File::open(output.join(".duplicates.json")).unwrap(),
    ))
    .unwrap();
    assert_eq!(duplicates.files["parent.ymap"].len(), 1);
    assert_eq!(duplicates.files["parent.ymap"][0].guid, 100);
    assert_eq!(duplicates.files["parent.ymap"][0].ignored[0].reason, "identical_duplicate");
    fs::copy(&sample, mods.join("resource_b___parent.ymap.xml")).unwrap();
    fs::copy(&sample, mods.join("resource_a___parent.ymap.xml")).unwrap();
    merge.run().unwrap();
    assert!(!output.join("parent.ymap.xml").exists());
    fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn merge_deletes_omitted_entities_and_detaches_dependent_children() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    let staging = std::env::temp_dir().join(format!("mlo_parent_refs_{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    let samples = base.join("docs/sample/parent_refs");
    let vanilla_dir = staging.join("vanilla");
    let mod_dir = staging.join("mods");
    let mod_ymap_dir = staging.join("mod-binaries");
    let output = staging.join("output/merged.xml");
    fs::create_dir_all(&vanilla_dir).unwrap();
    fs::create_dir_all(&mod_dir).unwrap();
    fs::create_dir_all(&mod_ymap_dir).unwrap();
    fs::copy(samples.join("vanilla_parent.ymap.xml"), vanilla_dir.join("parent.ymap.xml")).unwrap();
    fs::copy(samples.join("child.ymap.xml"), vanilla_dir.join("dependent.ymap.xml")).unwrap();
    fs::copy(
      samples.join("resource_a_parent.ymap.xml"),
      mod_dir.join("resource_a___parent.ymap.xml"),
    )
    .unwrap();
    let mut deleted_parent = parse_ymap_xml(&samples.join("vanilla_parent.ymap.xml")).unwrap();
    deleted_parent.entity_map.shift_remove(&200);
    let deleted_parent: XmlYmap = deleted_parent.into();
    fs::write(
      mod_dir.join("resource_b___parent.ymap.xml"),
      quick_xml::se::to_string(&deleted_parent).unwrap(),
    )
    .unwrap();
    fs::copy(samples.join("child.ymap.xml"), mod_dir.join("resource_a___child.ymap.xml")).unwrap();
    fs::write(mod_ymap_dir.join("resource_a___child.ymap"), b"placeholder clone binary").unwrap();
    let (cache, _) = VanillaParentCache::update(&vanilla_dir).unwrap();
    assert_eq!(
      cache
        .children(&vanilla_dir, &[reference_hash("parent")].into_iter().collect())
        .iter()
        .map(|(_, path)| path.file_name().unwrap().to_str().unwrap())
        .collect::<Vec<_>>(),
      ["dependent.ymap.xml"]
    );
    MergeYmap {
      vanilla_dir,
      mod_dir,
      mod_ymap_dir,
      output_dir: output.clone(),
      rebuild_all: false,
      blacklist_config: None,
    }
    .run()
    .unwrap();
    let parent = OriginalMap::load(&output.join("parent.ymap.xml")).unwrap();
    assert_eq!(parent.entities.iter().map(|entity| entity.guid).collect::<Vec<_>>(), [100]);
    let clone = output.join("clone/child.ymap");
    assert!(!clone.exists(), "a child of a deleted parent must be rebuilt");
    assert!(!fs::read_to_string(output.join("_copy_targets.txt")).unwrap().contains("child"));
    for name in ["child.ymap.xml", "dependent.ymap.xml"] {
      let child = OriginalMap::load(&output.join(name)).unwrap();
      assert_eq!(child.entities.len(), 1);
      assert_eq!(child.entities[0].parent_index, -1);
      assert_eq!(child.entities[0].flags & 8, 0);
      assert_eq!(child.entities[0].lod_level, "LODTYPES_DEPTH_ORPHANHD");
    }
    fs::remove_dir_all(staging).unwrap();
  }

  #[test]
  #[ignore = "requires local vanilla XML, extracted mods, and the original merge log"]
  fn merge_logged_unsupported_ymaps() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    let old_log =
      fs::read_to_string(base.join("asset/log/mlo_merger_20261002_154241.log")).unwrap();
    let mut current = None;
    let mut targets = BTreeSet::new();
    for line in old_log.lines() {
      if let Some((_, path)) = line.split_once("Processing YMAP: ") {
        current = Some(Path::new(path).file_name().unwrap().to_str().unwrap().to_string());
      }
      if ["parent(", "physics_dictionaries(", "instanced_data("]
        .iter()
        .any(|field| line.contains(&format!("skip unsupported changes: {field}")))
      {
        targets.insert(current.clone().expect("warning without YMAP context"));
      }
    }
    assert!(!targets.is_empty(), "no unsupported-field fixtures found");
    let staging = std::env::temp_dir().join(format!("mlo_merge_metadata_{}", std::process::id()));
    let vanilla_dir = staging.join("vanilla.xml");
    let mod_dir = staging.join("mod.xml");
    fs::create_dir_all(&vanilla_dir).unwrap();
    fs::create_dir_all(&mod_dir).unwrap();
    let output_dir = base.join("asset/merge_validation/unsupported_fields/merged.xml");
    let report_dir = output_dir.parent().unwrap();
    fs::create_dir_all(report_dir).unwrap();
    simplelog::WriteLogger::init(
      log::LevelFilter::Info,
      simplelog::Config::default(),
      fs::File::create(report_dir.join("merge.log")).unwrap(),
    )
    .unwrap();
    for name in &targets {
      fs::copy(base.join("asset/vanilla/ymap.xml").join(name), vanilla_dir.join(name))
        .unwrap_or_else(|error| panic!("Native vanilla XML missing for {name}: {error}"));
    }
    let sources = collect_modded_ymaps_map(&base.join("asset/extracted.xml")).unwrap();
    let mut mod_count = 0;
    for name in &targets {
      let references = sources.get(name).unwrap_or_else(|| panic!("no mod references for {name}"));
      for source in references {
        fs::copy(&source.mod_ymap_path, mod_dir.join(source.mod_ymap_path.file_name().unwrap()))
          .unwrap();
        mod_count += 1;
      }
    }
    let runner = MergeYmap {
      vanilla_dir,
      mod_dir,
      mod_ymap_dir: base.join("asset/extracted"),
      output_dir: output_dir.clone(),
      rebuild_all: true,
      blacklist_config: Some(base.join("asset/blacklist.toml")),
    };
    runner.run().unwrap();
    let mut records = Vec::new();
    for name in &targets {
      let original = parse_ymap_xml(&runner.vanilla_dir.join(name)).unwrap();
      let mods = sources[name]
        .iter()
        .map(|source| parse_ymap_xml(&source.mod_ymap_path).unwrap())
        .collect::<Vec<_>>();
      let merged = parse_ymap_xml(&output_dir.join(name)).unwrap();
      let expected_parent = mods
        .iter()
        .find(|modified| reference_hash(&modified.parent) != reference_hash(&original.parent))
        .map(|modified| &modified.parent)
        .unwrap_or(&original.parent);
      assert_eq!(reference_hash(&merged.parent), reference_hash(expected_parent), "parent: {name}");
      let original_refs = original
        .physics_dictionaries
        .iter()
        .map(|name| reference_hash(name))
        .collect::<HashSet<_>>();
      let modified_refs = mods
        .iter()
        .map(|modified| {
          modified
            .physics_dictionaries
            .iter()
            .map(|name| reference_hash(name))
            .collect::<HashSet<_>>()
        })
        .collect::<Vec<_>>();
      let mut expected_refs = original_refs
        .iter()
        .filter(|hash| modified_refs.iter().all(|refs| refs.contains(hash)))
        .copied()
        .collect::<HashSet<_>>();
      for refs in &modified_refs {
        expected_refs.extend(refs.difference(&original_refs).copied());
      }
      let actual_refs =
        merged.physics_dictionaries.iter().map(|name| reference_hash(name)).collect::<HashSet<_>>();
      assert_eq!(actual_refs, expected_refs, "physics dictionaries: {name}");
      assert_eq!(
        actual_refs.len(),
        merged.physics_dictionaries.len(),
        "duplicate dictionaries: {name}"
      );
      verify_grass(&original, &mods, &merged, name);
      let instances = merged
        .instanced_data
        .grass_instance_list
        .iter()
        .map(|batch| batch.instances.len())
        .sum::<usize>();
      records.push(serde_json::json!({
        "ymap": name, "mods": sources[name].iter().map(|source| &source.mod_name).collect::<Vec<_>>(),
        "parent": merged.parent, "physics_dictionaries": merged.physics_dictionaries.len(),
        "grass_batches": merged.instanced_data.grass_instance_list.len(), "grass_instances": instances,
      }));
    }
    fs::write(report_dir.join("results.json"), serde_json::to_vec_pretty(&records).unwrap())
      .unwrap();
    let merge_log = fs::read_to_string(report_dir.join("merge.log")).unwrap();
    assert!(
      !merge_log.contains("skip unsupported changes"),
      "unsupported changes remain in targeted merge log"
    );
    fs::remove_dir_all(staging).unwrap();
    eprintln!(
      "Verified {} targeted YMAPs across {mod_count} mod references; output: {}",
      targets.len(),
      output_dir.display()
    );
  }

  fn positions(batch: &GrassInstanceBatch) -> BTreeMap<Vec<u32>, GrassInstance> {
    batch
      .instances
      .iter()
      .map(|instance| {
        (
          instance
            .position
            .iter()
            .map(|value| if *value == 0.0 { 0 } else { value.to_bits() })
            .collect(),
          instance.clone(),
        )
      })
      .collect()
  }

  fn verify_grass(
    original: &Ymap,
    mods: &[Ymap],
    merged: &Ymap,
    name: &str,
  ) {
    let original_batches = original
      .instanced_data
      .grass_instance_list
      .iter()
      .map(|batch| (BatchKey::from_batch(batch), positions(batch)))
      .collect::<HashMap<_, _>>();
    let mod_batches = mods
      .iter()
      .map(|modified| {
        modified
          .instanced_data
          .grass_instance_list
          .iter()
          .map(|batch| (BatchKey::from_batch(batch), positions(batch)))
          .collect::<HashMap<_, _>>()
      })
      .collect::<Vec<_>>();
    let keys = original_batches
      .keys()
      .chain(mod_batches.iter().flat_map(|batches| batches.keys()))
      .cloned()
      .collect::<HashSet<_>>();
    let mut expected = HashMap::new();
    for key in keys {
      let before = original_batches.get(&key);
      if before.is_some() && mod_batches.iter().any(|batches| !batches.contains_key(&key)) {
        continue;
      }
      let empty = BTreeMap::new();
      let before = before.unwrap_or(&empty);
      let mut instances = before.clone();
      instances.retain(|position, _| {
        mod_batches.iter().all(|batches| {
          batches.get(&key).is_some_and(|instances| instances.contains_key(position))
        })
      });
      let removed = before
        .keys()
        .filter(|position| !instances.contains_key(*position))
        .cloned()
        .collect::<HashSet<_>>();
      let mut selected = HashSet::new();
      for batches in &mod_batches {
        if let Some(modified) = batches.get(&key) {
          for (position, instance) in modified {
            if !removed.contains(position)
              && before.get(position) != Some(instance)
              && selected.insert(position.clone())
            {
              instances.insert(position.clone(), instance.clone());
            }
          }
        }
      }
      expected.insert(key, instances);
    }
    let actual = merged
      .instanced_data
      .grass_instance_list
      .iter()
      .map(|batch| (BatchKey::from_batch(batch), positions(batch)))
      .collect::<HashMap<_, _>>();
    assert!(
      actual == expected,
      "grass deltas differ for {name}: actual {} batches, expected {}",
      actual.len(),
      expected.len()
    );
    assert_eq!(
      actual.len(),
      merged.instanced_data.grass_instance_list.len(),
      "duplicate grass batches: {name}"
    );
  }
}
