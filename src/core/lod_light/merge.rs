use super::{
  export::uncached_snapshot,
  io::{Fingerprint, reject_symlinks, safe_relative},
  resource::ResourceFile,
  *,
};
use crate::core::{
  format::{
    gamefile::{
      meta_resource::{MetaResource, MetaSchemaCatalog},
      resource_file::Rsc7Resource,
    },
    ymap::{
      binary::write_ymap,
      diff::reference_hash,
      model::{YmapDistantLodLight, YmapDistantLodLights, YmapLodLight},
    },
  },
  source_cache::MergeSourceFile,
};
use std::{
  collections::{BTreeMap, BTreeSet},
  fs,
  io::BufReader,
  path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct PreparedEdits {
  pub sources: Vec<MergeSourceFile>,
  pub dependencies: Vec<PathBuf>,
  pub names: BTreeSet<String>,
  pub omitted: BTreeSet<PathBuf>,
  edit_paths: Vec<(PathBuf, BTreeSet<String>)>,
  expected_inputs: BTreeMap<PathBuf, (String, u64)>,
  outputs: BTreeMap<String, Output>,
  #[serde(skip)]
  pub(super) fingerprints: BTreeMap<PathBuf, Fingerprint>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Output {
  template: LoadedMap,
  templates: Vec<LoadedMap>,
  lod: Option<Vec<YmapLodLight>>,
  distant: Option<YmapDistantLodLights>,
}

#[derive(Clone, PartialEq, serde::Serialize)]
struct Pair {
  distant: YmapDistantLodLight,
  lod: BTreeMap<String, YmapLodLight>,
}

impl PreparedEdits {
  pub fn load(
    source: &Path,
    cache: &Path,
    overwrite: &Path,
  ) -> Result<Self> {
    let mut prepared = Self::default();
    if !overwrite.exists() {
      return Ok(prepared);
    }
    reject_symlinks(overwrite)?;
    let mut documents = Vec::new();
    for entry in WalkDir::new(overwrite).follow_links(false).sort_by_file_name() {
      let entry = entry?;
      if entry.file_type().is_symlink() {
        return Err("Symlink in light overwrite directory".into());
      }
      if !entry.file_type().is_file()
        || !entry.file_name().to_string_lossy().ends_with(".diff.json")
      {
        continue;
      }
      let before = Fingerprint::read(entry.path(), None, true)?;
      let edit: YmapLightEditDocument =
        serde_json::from_reader(BufReader::new(fs::File::open(entry.path())?))?;
      if before != Fingerprint::read(entry.path(), None, true)? {
        return Err("Light diff changed while reading".into());
      }
      prepared.expected_inputs.insert(entry.path().to_path_buf(), (before.sha256, before.size));
      safe_relative(&edit.resource)?;
      safe_relative(&edit.path)?;
      let expected = PathBuf::from(&edit.resource).join(format!("{}.diff.json", edit.path));
      if entry.path().strip_prefix(overwrite)? != expected {
        return Err(
          format!("Light diff path and target disagree: {}", entry.path().display()).into(),
        );
      }
      prepared.dependencies.push(entry.path().to_path_buf());
      prepared.edit_paths.push((
        entry.path().to_path_buf(),
        edit.files.iter().map(|file| basename(&file.path)).collect::<Result<_>>()?,
      ));
      documents.push(edit);
    }
    if documents.is_empty() {
      return Ok(prepared);
    }
    let inventory = inventory(source, cache)?;
    let mut originals = BTreeMap::<String, BTreeMap<String, LoadedMap>>::new();
    let mut masks = BTreeMap::<(String, String, bool), BTreeSet<usize>>::new();
    for edit in documents {
      let files =
        inventory.get(&edit.resource).ok_or("Light edit targets unknown/excluded resource")?;
      let (export, maps) = uncached_snapshot(&edit.resource, &edit.path, files)?;
      prepared.remember(&export, files)?;
      let mut validated = maps.clone();
      edits::apply(&export, &edit, &mut validated)?;
      for operation in &edit.operations {
        let LightOperation::Remove {
          light_id,
        } = operation;
        let light =
          export.lights.iter().find(|light| light.id == *light_id).ok_or("Unknown removal")?;
        for (reference, distant) in std::iter::once((&light.distant_ref, true))
          .chain(light.lod.iter().map(|view| (&view.reference, false)))
        {
          let file = export
            .files
            .iter()
            .find(|file| file.file_id == reference.file_id)
            .ok_or("Missing light file reference")?;
          masks
            .entry((edit.resource.clone(), file.path.clone(), distant))
            .or_default()
            .insert(reference.index);
        }
      }
      for (path, map) in maps {
        if !map.model.lod_lights.is_empty() || !map.model.distant_lod_lights.items.is_empty() {
          prepared.names.insert(basename(&path)?);
        }
        originals.entry(edit.resource.clone()).or_default().insert(path, map);
      }
    }
    if masks.is_empty() {
      return Ok(Self::default());
    }
    loop {
      let before = prepared.names.len();
      for (resource, files) in &inventory {
        for target in files
          .keys()
          .filter(|path| prepared.names.contains(&basename(path).unwrap_or_default()))
          .cloned()
          .collect::<Vec<_>>()
        {
          if originals.get(resource).is_some_and(|maps| maps.contains_key(&target)) {
            continue;
          }
          let (export, maps) = uncached_snapshot(resource, &target, files)?;
          prepared.remember(&export, files)?;
          for (path, map) in maps {
            if !map.model.lod_lights.is_empty() || !map.model.distant_lod_lights.items.is_empty() {
              prepared.names.insert(basename(&path)?);
            }
            originals.entry(resource.clone()).or_default().insert(path, map);
          }
        }
      }
      if before == prepared.names.len() {
        break;
      }
    }
    let mut edited = originals.clone();
    for ((resource, path, distant), removals) in &masks {
      let map = edited
        .get_mut(resource)
        .and_then(|maps| maps.get_mut(path))
        .ok_or("Missing edit dependency")?;
      let mut index = 0;
      if *distant {
        map.model.distant_lod_lights.items.retain(|_| {
          let retain = !removals.contains(&index);
          index += 1;
          retain
        });
      } else {
        map.model.lod_lights.retain(|_| {
          let retain = !removals.contains(&index);
          index += 1;
          retain
        });
      }
    }
    prepared.compose(&originals, &edited)?;
    for (resource, maps) in &originals {
      for path in
        maps.keys().filter(|path| prepared.names.contains(&basename(path).unwrap_or_default()))
      {
        let file = &inventory[resource][path];
        prepared.sources.push(MergeSourceFile {
          resource: resource.clone(),
          path: file.path.clone(),
          original_path: file.original.clone(),
          file_name: basename(path)?,
        });
        prepared.dependencies.push(file.path.clone());
        prepared.omitted.insert(file.original.clone());
      }
    }
    prepared.dependencies.sort();
    prepared.dependencies.dedup();
    Ok(prepared)
  }

  fn remember(
    &mut self,
    export: &YmapLightDocument,
    files: &BTreeMap<String, ResourceFile>,
  ) -> Result<()> {
    for file in &export.files {
      let expected = (file.sha256.clone(), file.size);
      let path = files[&file.path].path.clone();
      if self.expected_inputs.get(&path).is_some_and(|previous| previous != &expected) {
        return Err("Light source changed between linked snapshots".into());
      }
      self.expected_inputs.insert(path, expected);
    }
    Ok(())
  }

  pub fn verify(&self) -> Result<()> {
    for (path, (sha256, size)) in &self.expected_inputs {
      let actual = Fingerprint::read(path, self.fingerprints.get(path), false)?;
      if actual.sha256 != *sha256 || actual.size != *size {
        return Err(
          format!("Light edit input changed before publication: {}", path.display()).into(),
        );
      }
    }
    Ok(())
  }

  pub(crate) fn dependency_hash(
    &self,
    path: &Path,
  ) -> Result<String> {
    Ok(Fingerprint::read(path, self.fingerprints.get(path), false)?.sha256)
  }

  fn compose(
    &mut self,
    originals: &BTreeMap<String, BTreeMap<String, LoadedMap>>,
    edited: &BTreeMap<String, BTreeMap<String, LoadedMap>>,
  ) -> Result<()> {
    let mut combined = BTreeMap::<String, Vec<Pair>>::new();
    let mut layouts = BTreeMap::<String, BTreeSet<String>>::new();
    for (resource, maps) in edited {
      for (path, parent) in maps {
        if originals[resource][path].model.distant_lod_lights.items.is_empty() {
          continue;
        }
        let parent_name = basename(path)?;
        let children = maps
          .iter()
          .filter(|(child_path, child)| {
            reference_hash(&child.model.parent) == resource::file_hash(path).unwrap_or(0)
              && !originals[resource][*child_path].model.lod_lights.is_empty()
          })
          .collect::<Vec<_>>();
        let child_names =
          children.iter().map(|(path, _)| basename(path)).collect::<Result<BTreeSet<_>>>()?;
        if children.is_empty()
          || children.iter().any(|(_, child)| {
            child.model.lod_lights.len() != parent.model.distant_lod_lights.items.len()
          })
        {
          return Err(format!("Cannot safely compose light arrays in {resource}/{path}").into());
        }
        if layouts.get(&parent_name).is_some_and(|layout| *layout != child_names) {
          return Err(
            format!("Incompatible cross-resource light child layout: {parent_name}").into(),
          );
        }
        layouts.insert(parent_name.clone(), child_names);
        self
          .outputs
          .entry(parent_name.clone())
          .and_modify(|output| output.templates.push(parent.clone()))
          .or_insert_with(|| Output {
            template: parent.clone(),
            templates: vec![parent.clone()],
            lod: None,
            distant: Some(parent.model.distant_lod_lights.clone()),
          });
        for (child_path, child) in &children {
          self
            .outputs
            .entry(basename(child_path)?)
            .and_modify(|output| output.templates.push((*child).clone()))
            .or_insert_with(|| Output {
              template: (*child).clone(),
              templates: vec![(*child).clone()],
              lod: Some(Vec::new()),
              distant: None,
            });
        }
        let mut pairs = Vec::new();
        for (index, distant) in parent.model.distant_lod_lights.items.iter().enumerate() {
          let lod = children
            .iter()
            .map(|(path, child)| Ok((basename(path)?, child.model.lod_lights[index].clone())))
            .collect::<Result<BTreeMap<_, _>>>()?;
          let flags = lod
            .values()
            .map(|light| light.time_and_state_flags.parse::<u32>())
            .collect::<std::result::Result<Vec<_>, _>>()?;
          if flags.iter().any(|flag| flag & (1 << 24) != flags[0] & (1 << 24)) {
            return Err("Shared parent streetlight classification differs".into());
          }
          pairs.push(Pair {
            distant: distant.clone(),
            lod,
          });
        }
        union_occurrences(combined.entry(parent_name).or_default(), &pairs)?;
      }
    }
    for (parent, pairs) in combined {
      let output = self.outputs.get_mut(&parent).ok_or("Missing composed distant output")?;
      let distant = output.distant.as_mut().ok_or("Missing distant array")?;
      distant.items = pairs.iter().map(|pair| pair.distant.clone()).collect();
      distant.num_street_lights = pairs
        .iter()
        .filter(|pair| {
          pair.lod.values().next().is_some_and(|light| {
            light.time_and_state_flags.parse::<u32>().unwrap_or(0) & (1 << 24) != 0
          })
        })
        .count()
        .try_into()?;
      for child in &layouts[&parent] {
        let output = self.outputs.get_mut(child).ok_or("Missing composed child")?;
        if output.lod.as_ref().is_some_and(|lod| !lod.is_empty()) {
          return Err("Child belongs to multiple distant outputs".into());
        }
        output.lod = Some(pairs.iter().map(|pair| pair.lod[child].clone()).collect());
      }
    }
    Ok(())
  }

  pub fn connect(
    &self,
    graph: &mut BTreeMap<String, BTreeSet<String>>,
  ) {
    for (name, output) in &self.outputs {
      graph.entry(name.clone()).or_default();
      let parent_hash = reference_hash(&output.template.model.parent);
      if let Some(parent) =
        self.names.iter().find(|parent| resource::file_hash(parent).ok() == Some(parent_hash))
      {
        graph.entry(name.clone()).or_default().insert(parent.clone());
        graph.entry(parent.clone()).or_default().insert(name.clone());
      }
    }
  }

  pub fn dependencies_for(
    &self,
    names: &BTreeSet<String>,
  ) -> BTreeSet<PathBuf> {
    self
      .edit_paths
      .iter()
      .filter(|(_, files)| !files.is_disjoint(names))
      .map(|(path, _)| path.clone())
      .chain(
        self
          .sources
          .iter()
          .filter(|source| names.contains(&source.file_name))
          .map(|source| source.path.clone()),
      )
      .collect()
  }

  pub fn patch(
    &self,
    names: &BTreeSet<String>,
    directory: &Path,
    baselines: &BTreeMap<String, &PathBuf>,
  ) -> Result<()> {
    if !names.iter().any(|name| self.outputs.contains_key(name)) {
      return Ok(());
    }
    let mut catalog = MetaSchemaCatalog::default();
    for source in &self.sources {
      let bytes = fs::read(&source.path)?;
      let resource = Rsc7Resource::decode(&bytes)
        .map_err(|_| "Light editing currently requires RSC7, not PSO")?;
      catalog.add_resource(&MetaResource::parse(&resource)?);
    }
    fs::create_dir_all(directory)?;
    for (name, output) in self.outputs.iter().filter(|(name, _)| names.contains(*name)) {
      let path = directory.join(name);
      let existing = if path.is_file() { Some(&path) } else { baselines.get(name).copied() };
      if existing.is_none() {
        for candidate in &output.templates {
          let mut normalized = candidate.model.clone();
          normalized.lod_lights = output.template.model.lod_lights.clone();
          normalized.distant_lod_lights = output.template.model.distant_lod_lights.clone();
          normalized.block = output.template.model.block.clone();
          if normalized != output.template.model || candidate.entities != output.template.entities {
            return Err(
              format!(
                "Custom light YMAP has competing non-light data without a vanilla baseline: {name}"
              )
              .into(),
            );
          }
          if candidate.model.distant_lod_lights.category
            != output.template.model.distant_lod_lights.category
          {
            return Err(format!("Custom distant light categories disagree: {name}").into());
          }
        }
      }
      let mut map = if let Some(existing) = existing {
        read_map(&ResourceFile {
          path: existing.clone(),
          original: existing.clone(),
          parent_hint: None,
        })?
      } else {
        output.template.clone()
      };
      if let Some(lod) = &output.lod {
        map.model.lod_lights = lod.clone();
      }
      if let Some(distant) = &output.distant {
        map.model.distant_lod_lights.items = distant.items.clone();
        map.model.distant_lod_lights.num_street_lights = distant.num_street_lights;
      }
      fs::write(&path, write_ymap(&map.model, &map.entities, &catalog)?)?;
    }
    Ok(())
  }
}

fn basename(path: &str) -> Result<String> {
  Ok(
    Path::new(path)
      .file_name()
      .and_then(|name| name.to_str())
      .ok_or("Invalid light basename")?
      .to_ascii_lowercase(),
  )
}

/// Combines resource providers by maximum exact occurrence count, not by direction/hash.
fn union_occurrences(
  combined: &mut Vec<Pair>,
  incoming: &[Pair],
) -> Result<()> {
  let mut counts = BTreeMap::<Vec<u8>, usize>::new();
  let mut identities = BTreeMap::<Vec<u8>, BTreeSet<Vec<u8>>>::new();
  for pair in combined.iter() {
    let key = serde_json::to_vec(pair)?;
    identities.entry(pair_identity(pair)?).or_default().insert(key.clone());
    *counts.entry(key).or_default() += 1;
  }
  let mut seen = BTreeMap::<Vec<u8>, usize>::new();
  for pair in incoming {
    let key = serde_json::to_vec(pair)?;
    if identities.get(&pair_identity(pair)?).is_some_and(|previous| !previous.contains(&key)) {
      return Err("Ambiguous cross-resource light properties at the same position/hashes; no priority policy is inferred".into());
    }
    let count = seen.entry(key.clone()).or_default();
    *count += 1;
    if *count > counts.get(&key).copied().unwrap_or(0) {
      combined.push(pair.clone());
    }
  }
  Ok(())
}

fn pair_identity(pair: &Pair) -> Result<Vec<u8>> {
  let hashes = pair
    .lod
    .iter()
    .map(|(path, light)| Ok((path, light.hash.parse::<u32>()?)))
    .collect::<Result<BTreeMap<_, _>>>()?;
  Ok(serde_json::to_vec(&(snapshot::parse_position(&pair.distant.position)?, hashes))?)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn resource_union_retains_other_provider_and_duplicate_multiplicity() {
    let pair = Pair {
      distant: YmapDistantLodLight {
        position: "0 0 0".into(),
        rgbi: "1".into(),
      },
      lod: BTreeMap::new(),
    };
    let mut combined = Vec::new();
    union_occurrences(&mut combined, &[pair.clone(), pair.clone()]).unwrap();
    union_occurrences(&mut combined, std::slice::from_ref(&pair)).unwrap();
    assert_eq!(combined.len(), 2);
    let mut removed_a = Vec::new();
    union_occurrences(&mut removed_a, &[]).unwrap();
    union_occurrences(&mut removed_a, std::slice::from_ref(&pair)).unwrap();
    assert_eq!(removed_a.len(), 1);
  }

  #[test]
  fn conflicting_light_properties_are_not_silently_duplicated() {
    let pair = Pair {
      distant: YmapDistantLodLight {
        position: "0 0 0".into(),
        rgbi: "1".into(),
      },
      lod: BTreeMap::new(),
    };
    let mut changed = pair.clone();
    changed.distant.rgbi = "2".into();
    assert!(union_occurrences(&mut vec![pair], &[changed]).is_err());
  }
}
