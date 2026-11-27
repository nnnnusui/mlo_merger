use std::{
  collections::{BTreeMap, BTreeSet, HashMap},
  path::PathBuf,
  sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};

use super::{Read, ReadTarget, Selected};
use crate::{
  app::map::source_cache::inventoried_files,
  common::Result,
  core::format::ymap::{
    binary::ymap_to_model,
    model::Ymap,
    reference_hash,
    relation::{is_local_parent, runtime_entity_indices},
  },
  platform::fs,
};

/// One entity with a non-negative LOD parent index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct EntityLodParent {
  /// Index into this document's `model.entities`.
  pub child_entity_index: usize,
  /// The resolved parent, or `None` when the reference could not be resolved.
  pub parent: Option<EntityLodParentRef>,
}

/// A parent entity in this YMAP or its declared parent YMAP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct EntityLodParentRef {
  /// Whether the parent is in this YMAP or its declared parent.
  pub scope: EntityLodParentScope,
  /// Index into the referenced YMAP's `model.entities`, not its runtime entity order.
  pub entity_index: usize,
}

/// Which YMAP contains a resolved parent entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub enum EntityLodParentScope {
  /// This document's YMAP.
  #[serde(rename = "self")]
  SelfMap,
  /// The YMAP named by this document's `model.parent`.
  Parent,
}

type Candidates = BTreeMap<u32, BTreeMap<String, Vec<PathBuf>>>;

/// Parent maps are read even when they were not selected for output.
pub(super) struct ParentMaps<'a> {
  resources: HashMap<String, Candidates>,
  vanilla: Candidates,
  names: &'a HashMap<u32, String>,
  loaded: Mutex<HashMap<PathBuf, Arc<Ymap>>>,
}

impl<'a> ParentMaps<'a> {
  pub(super) fn new(
    read: &Read,
    names: &'a HashMap<u32, String>,
  ) -> Result<Self> {
    let mut resources = HashMap::<String, Candidates>::new();
    if !matches!(read.target, ReadTarget::Vanilla) {
      for file in inventoried_files(&read.source_cache_dir, &read.source_dir)? {
        insert(resources.entry(file.resource).or_default(), &file.file_name, file.path);
      }
    }
    let mut vanilla = Candidates::new();
    if fs::is_file(crate::core::game_archive::manifest_path(&read.vanilla_dir)) {
      let (manifest, version) = read.vanilla_stage()?;
      for (name, file) in manifest.resolve_version(&version)? {
        insert(&mut vanilla, &name, file.path(&read.vanilla_dir)?);
      }
    }
    Ok(Self {
      resources,
      vanilla,
      names,
      loaded: Mutex::new(HashMap::new()),
    })
  }

  pub(super) fn resolve(
    &self,
    selected: &Selected,
    model: &Ymap,
  ) -> std::result::Result<Vec<EntityLodParent>, String> {
    let own_indices = runtime_entity_indices(&model.entities);
    let own = own_indices.iter().map(|&index| &model.entities[index]).collect::<Vec<_>>();
    let needs_parent = model
      .entities
      .iter()
      .any(|entity| entity.parent_index >= 0 && !is_local_parent(entity, &own));
    let external = if needs_parent { self.parent_map(selected, &model.parent)? } else { None };
    let external_indices = external.as_ref().map(|(_, map)| runtime_entity_indices(&map.entities));
    let mut unresolved = 0;
    let result = model
      .entities
      .iter()
      .enumerate()
      .filter_map(|(child_entity_index, entity)| {
        let index = usize::try_from(entity.parent_index).ok()?;
        let parent = if is_local_parent(entity, &own) {
          own_indices.get(index).map(|&entity_index| EntityLodParentRef {
            scope: EntityLodParentScope::SelfMap,
            entity_index,
          })
        } else {
          external_indices.as_ref().and_then(|indices| indices.get(index)).map(|&entity_index| {
            EntityLodParentRef {
              scope: EntityLodParentScope::Parent,
              entity_index,
            }
          })
        };
        if parent.is_none() {
          unresolved += 1;
        }
        Some(EntityLodParent {
          child_entity_index,
          parent,
        })
      })
      .collect();
    if unresolved > 0 {
      tracing::warn!(
        "{}: {unresolved} entity LOD parent references could not be resolved (parent YMAP: {})",
        selected.input.display(),
        external.as_ref().map_or(model.parent.as_str(), |(name, _)| name.as_str())
      );
    }
    Ok(result)
  }

  fn parent_map(
    &self,
    selected: &Selected,
    name: &str,
  ) -> std::result::Result<Option<(String, Arc<Ymap>)>, String> {
    let hash = reference_hash(name);
    if hash == 0 {
      return Ok(None);
    }
    let resource = selected.resource_id.as_ref().and_then(|id| self.resources.get(id));
    let resource = resource.and_then(|files| files.get(&hash));
    let vanilla = self.vanilla.get(&hash);
    let candidates =
      resource.into_iter().chain(vanilla).flat_map(|files| files.keys()).collect::<BTreeSet<_>>();
    if candidates.len() > 1 {
      tracing::warn!("{}: parent {name} matches several YMAP names", selected.input.display());
      return Ok(None);
    }
    let Some(name) = candidates.first() else {
      return Ok(None);
    };
    let paths = resource.and_then(|files| files.get(*name)).or_else(|| vanilla?.get(*name));
    let Some([path]) = paths.map(Vec::as_slice) else {
      tracing::warn!("{}: parent {name} matches several YMAP files", selected.input.display());
      return Ok(None);
    };
    let loaded = self.loaded.lock().map_err(|error| error.to_string())?;
    if let Some(model) = loaded.get(path) {
      return Ok(Some(((*name).clone(), model.clone())));
    }
    drop(loaded);
    let bytes =
      fs::read(path).map_err(|error| format!("Cannot open {}: {error}", path.display()))?;
    let model = ymap_to_model(&bytes, self.names)
      .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    let model = Arc::new(model);
    let mut loaded = self.loaded.lock().map_err(|error| error.to_string())?;
    Ok(Some(((*name).clone(), loaded.entry(path.clone()).or_insert(model).clone())))
  }
}

fn insert(
  candidates: &mut Candidates,
  file_name: &str,
  path: PathBuf,
) {
  let name = file_name.to_ascii_lowercase();
  if let Some(stem) = name.strip_suffix(".ymap") {
    candidates.entry(reference_hash(stem)).or_default().entry(name.clone()).or_default().push(path);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app::map::read::ReadOrigin;

  #[derive(Clone)]
  struct LogWriter(Arc<Mutex<Vec<u8>>>);

  impl std::io::Write for LogWriter {
    fn write(
      &mut self,
      bytes: &[u8],
    ) -> std::io::Result<usize> {
      self.0.lock().unwrap().extend_from_slice(bytes);
      Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
      Ok(())
    }
  }

  #[test]
  fn warnings_name_the_resolved_file_or_keep_the_unresolved_reference() {
    let names = HashMap::new();
    let path = PathBuf::from("parent.ymap");
    let mut model =
      crate::core::format::gamefile::test_support::sample_ymap("parent_refs/child.ymap");
    model.parent = format!("hash_{:08X}", reference_hash("parent"));
    model.entities.truncate(1);
    model.entities[0].parent_index = 99;
    model.entities[0].flags |= 8;
    let mut parents = ParentMaps {
      resources: HashMap::new(),
      vanilla: Candidates::new(),
      names: &names,
      loaded: Mutex::new(HashMap::from([(path.clone(), Arc::new(Ymap::empty()))])),
    };
    insert(&mut parents.vanilla, "parent.ymap", path);
    let selected = Selected {
      origin: ReadOrigin::Vanilla {
        version: "0000-base".into(),
        file_name: "child.ymap".into(),
      },
      resource_id: None,
      input: "child.ymap".into(),
      stored: "child.ymap".into(),
      output: "child.ymap.json".into(),
    };
    let log = Arc::new(Mutex::new(Vec::new()));
    let writer = LogWriter(log.clone());
    let subscriber = tracing_subscriber::fmt()
      .without_time()
      .with_ansi(false)
      .with_writer(move || writer.clone())
      .finish();
    tracing::subscriber::with_default(subscriber, || {
      assert!(parents.resolve(&selected, &model).unwrap()[0].parent.is_none());
      parents.vanilla.clear();
      assert!(parents.resolve(&selected, &model).unwrap()[0].parent.is_none());
    });
    let log = String::from_utf8(log.lock().unwrap().clone()).unwrap();
    assert!(log.contains("(parent YMAP: parent.ymap)"));
    assert!(log.contains(&format!("(parent YMAP: {})", model.parent)));
  }
}
