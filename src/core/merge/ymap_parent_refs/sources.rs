use super::*;

/// Original model, entity order, and optional XML retained for legacy clone patching.
pub(in crate::core::merge) struct OriginalMap {
  pub(in crate::core::merge) model: Ymap,
  pub(in crate::core::merge) entities: Vec<YmapEntity>,
  pub(in crate::core::merge) xml: String,
}

impl OriginalMap {
  /// Loads without losing duplicate GUID entries from the original entity array.
  pub(in crate::core::merge) fn load(path: &Path) -> io::Result<Self> {
    let xml = std::fs::read_to_string(path)?;
    Self::from_xml(xml).map_err(|error| invalid(&format!("{}: {error}", path.display())))
  }

  pub(in crate::core::merge) fn load_raw(path: &Path) -> io::Result<Self> {
    let (model, entities) = crate::core::format::gamefile::meta_xml::ymap_to_model_with_entities(
      &std::fs::read(path).map_err(|error| invalid(&format!("{}: {error}", path.display())))?,
      &HashMap::new(),
    )
    .map_err(|error| invalid(&format!("{}: {error}", path.display())))?;
    Ok(Self {
      model,
      entities,
      xml: String::new(),
    })
  }

  pub(in crate::core::merge) fn from_xml(xml: String) -> io::Result<Self> {
    let mut parsed: XmlYmap =
      quick_xml::de::from_str(&xml).map_err(|error| invalid(&error.to_string()))?;
    if let Some(error) =
      parsed.instanced_data.error.as_ref().filter(|error| !error.trim().is_empty())
    {
      return Err(invalid(&format!("instancedData error: {error}")));
    }
    let entities = std::mem::take(&mut parsed.entities.items)
      .into_iter()
      .map(YmapEntity::from)
      .collect::<Vec<_>>();
    let mut model: Ymap = parsed.into();
    model.entity_map = entities.iter().map(|entity| (entity.guid, entity.clone())).collect();
    Ok(Self {
      model,
      entities,
      xml,
    })
  }
}

/// Lazy original-map lookup: same-mod parent first, vanilla parent otherwise.
#[derive(Default)]
pub(in crate::core::merge) struct SourceMaps {
  paths: HashMap<(Option<String>, u32), OriginalMapSource>,
  loaded: HashMap<(Option<String>, u32), Rc<OriginalMap>>,
}

enum OriginalMapSource {
  Xml(PathBuf),
  RawYmap(PathBuf),
}

impl SourceMaps {
  /// Registers a map by its stream filename, not its potentially unresolved XML name.
  pub(in crate::core::merge) fn register(
    &mut self,
    namespace: Option<&str>,
    name: &str,
    path: PathBuf,
  ) -> io::Result<()> {
    let name = name.strip_suffix(".xml").unwrap_or(name);
    let name = name.strip_suffix(".ymap").unwrap_or(name);
    let key = (namespace.map(str::to_string), reference_hash(name));
    if self.paths.contains_key(&key) || self.loaded.contains_key(&key) {
      warn_duplicate_map(namespace, name, &path);
      return Ok(());
    }
    if !path.is_file() {
      warn_missing_map(namespace, name, &path);
      return Ok(());
    }
    self.paths.insert(key, OriginalMapSource::Xml(path));
    Ok(())
  }

  pub(in crate::core::merge) fn register_raw(
    &mut self,
    namespace: Option<&str>,
    name: &str,
    path: PathBuf,
  ) -> io::Result<()> {
    let name = name.strip_suffix(".xml").unwrap_or(name);
    let name = name.strip_suffix(".ymap").unwrap_or(name);
    let key = (namespace.map(str::to_string), reference_hash(name));
    if self.paths.contains_key(&key) || self.loaded.contains_key(&key) {
      warn_duplicate_map(namespace, name, &path);
      return Ok(());
    }
    if !path.is_file() {
      warn_missing_map(namespace, name, &path);
      return Ok(());
    }
    if self.paths.insert(key, OriginalMapSource::RawYmap(path)).is_some() {
      warn_duplicate_map(namespace, name, Path::new(name));
    }
    Ok(())
  }

  /// Loads a requested map and caches only maps actually involved in resolution.
  pub(in crate::core::merge) fn get(
    &mut self,
    namespace: Option<&str>,
    hash: u32,
  ) -> io::Result<Option<Rc<OriginalMap>>> {
    let preferred = (namespace.map(str::to_string), hash);
    let key = if self.paths.contains_key(&preferred) { preferred } else { (None, hash) };
    if let Some(map) = self.loaded.get(&key) {
      return Ok(Some(Rc::clone(map)));
    }
    let Some(source) = self.paths.get(&key) else {
      return Ok(None);
    };
    let path = match source {
      OriginalMapSource::Xml(path) | OriginalMapSource::RawYmap(path) => path,
    };
    if !path.is_file() {
      warn_missing_map(
        key.0.as_deref(),
        path.file_name().and_then(|name| name.to_str()).unwrap_or("<unknown>"),
        path,
      );
      self.paths.remove(&key);
      return Ok(None);
    }
    let map = Rc::new(match source {
      OriginalMapSource::Xml(path) => OriginalMap::load(path)?,
      OriginalMapSource::RawYmap(path) => OriginalMap::load_raw(path)?,
    });
    self.loaded.insert(key, Rc::clone(&map));
    Ok(Some(map))
  }

  /// Normalizes original parent indices into merge-stable handles.
  pub(in crate::core::merge) fn normalize(
    &mut self,
    original: &OriginalMap,
    namespace: Option<&str>,
    map_hash: u32,
    references: &mut ParentReferences,
  ) -> io::Result<(Ymap, Vec<YmapEntity>)> {
    references.register_entities(map_hash, &original.entities);
    let mut entities = original.entities.clone();
    let own_ids = references.entity_ids(map_hash, &original.entities)?;
    let original_order = runtime_entities(original.entities.iter());
    let own_layout = references.runtime_entity_ids(map_hash, &original.entities)?;
    let external = reference_hash(&original.model.parent);
    for (entity_index, entity) in entities.iter_mut().enumerate() {
      entity.guid = own_ids[entity_index];
      let index = entity.parent_index;
      if index < 0 {
        entity.parent_index = -1;
        continue;
      }
      let local = is_local_parent(entity, &original_order);
      let owner = if local { map_hash } else { external };
      let parent = if local { None } else { self.get(namespace, owner)? };
      let layout = if local {
        Some(own_layout.clone())
      } else {
        parent
          .as_ref()
          .map(|parent| {
            references.register_entities(owner, &parent.entities);
            references.runtime_entity_ids(owner, &parent.entities)
          })
          .transpose()?
      };
      entity.parent_index = match layout
        .as_deref()
        .map(|layout| references.capture(owner, layout, index))
        .transpose()
      {
        Ok(Some(handle)) => handle,
        Ok(None) => {
          log::warn!(
            "    Parent map {owner:08X} unavailable for entity {} index {index}; preserving only if parent remains unavailable",
            entity.guid
          );
          references.capture_unresolved(namespace, owner, None, index)?
        }
        Err(error) => {
          log::warn!(
            "    Unresolved parent for entity {}: {error}; changes to its parent layout will be rejected",
            entity.guid
          );
          references.capture_unresolved(namespace, owner, layout.as_deref(), index)?
        }
      };
    }
    let mut model = original.model.clone();
    model.entity_map = entities.iter().map(|entity| (entity.guid, entity.clone())).collect();
    Ok((model, entities))
  }
}

fn warn_duplicate_map(
  namespace: Option<&str>,
  name: &str,
  path: &Path,
) {
  let resource = namespace.unwrap_or("vanilla");
  log::warn!(
    "Duplicate original map {name} within resource {resource}; keeping the first registered file and skipping {}",
    path.display()
  );
}

fn warn_missing_map(
  namespace: Option<&str>,
  name: &str,
  path: &Path,
) {
  log::warn!(
    "Original map {name} is missing for resource {}; skipping {}",
    namespace.unwrap_or("vanilla"),
    path.display()
  );
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn duplicate_map_registration_keeps_the_first_file() {
    let root = std::env::temp_dir().join(format!("duplicate_ymap_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let first = root.join("first.ymap");
    let second = root.join("second.ymap");
    std::fs::write(&first, []).unwrap();
    std::fs::write(&second, []).unwrap();
    let mut sources = SourceMaps::default();
    sources.register_raw(Some("[gabz]/resource_a"), "map.ymap", first.clone()).unwrap();
    sources.register_raw(Some("[gabz]/resource_a"), "map.ymap", second).unwrap();

    let key = (Some("[gabz]/resource_a".into()), reference_hash("map"));
    assert!(
      matches!(sources.paths.get(&key), Some(OriginalMapSource::RawYmap(path)) if path == &first)
    );
    std::fs::remove_dir_all(root).unwrap();
  }

  #[test]
  fn missing_map_registration_is_skipped() {
    let mut sources = SourceMaps::default();
    sources
      .register_raw(
        Some("resource_missing"),
        "map.ymap",
        PathBuf::from("missing-resource/map.ymap"),
      )
      .unwrap();
    assert!(sources.paths.is_empty());
  }

  #[test]
  fn map_removed_after_registration_is_skipped_on_load() {
    let root = std::env::temp_dir().join(format!("removed_ymap_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("map.ymap");
    std::fs::write(&path, []).unwrap();
    let mut sources = SourceMaps::default();
    sources.register_raw(Some("resource"), "map.ymap", path.clone()).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(sources.get(Some("resource"), reference_hash("map")).unwrap().is_none());
    std::fs::remove_dir_all(root).unwrap();
  }
}
