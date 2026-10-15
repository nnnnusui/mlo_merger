//! Dumps entities that share a GUID within a single YMAP, for inspecting local assets.

use std::{
  collections::HashMap,
  fs,
  path::{Path, PathBuf},
};

use crate::core::{
  conversion::load_vanilla_hash_names,
  format::{
    gamefile::{meta::MetaResource, rsc7::Rsc7Resource},
    ymap::{binary::ymap_to_model, model::YmapEntity},
  },
};

#[derive(serde::Serialize)]
struct DuplicateGuidGroup {
  file: String,
  guid: u32,
  count: usize,
  /// `"x, y, z"` of each entity, in the same order as `entities`.
  positions: Vec<String>,
  /// Pairs of (index in `Ymap::entities`, entity).
  entities: Vec<(usize, YmapEntity)>,
}

#[test]
#[ignore = "requires local asset/vanilla-cache YMAPs"]
fn dump_vanilla_cache_duplicate_guid_entities() {
  dump_duplicate_guid_entities("vanilla-cache", true);
}

#[test]
#[ignore = "requires local asset/source-cache YMAPs"]
fn dump_source_cache_duplicate_guid_entities() {
  dump_duplicate_guid_entities("source-cache", true);
}

fn dump_duplicate_guid_entities(
  asset_dir: &str,
  include_zero_guid: bool,
) {
  let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
  let root = manifest.join("asset").join(asset_dir);
  let paths = ymap_paths(&root);

  // Names embedded in later files are not known yet while decoding, so archetype names are
  // resolved against the full set once every file has been read.
  let mut hash_names = load_vanilla_hash_names();
  let no_shared_names = HashMap::new();
  let mut groups = Vec::new();
  for path in paths {
    let bytes = fs::read(&path).unwrap();
    if let Ok(meta) =
      Rsc7Resource::decode(&bytes).and_then(|resource| MetaResource::parse(&resource))
    {
      hash_names.extend(meta.hash_names());
    }
    let Ok(ymap) = ymap_to_model(&bytes, &no_shared_names) else {
      continue;
    };
    let file = path.strip_prefix(&root).unwrap().display().to_string();
    groups.extend(duplicate_guid_groups(&file, ymap.entities, include_zero_guid));
  }
  for (_, entity) in groups.iter_mut().flat_map(|group| &mut group.entities) {
    resolve_hash_name(&mut entity.archetype_name, &hash_names);
  }

  let output = manifest.join("target/ymap_duplicate_guid").join(format!("{asset_dir}.json"));
  fs::create_dir_all(output.parent().unwrap()).unwrap();
  fs::write(&output, serde_json::to_string_pretty(&groups).unwrap()).unwrap();
  println!(
    "{} groups, {} entities -> {}",
    groups.len(),
    groups.iter().map(|group| group.count).sum::<usize>(),
    output.display()
  );
}

fn resolve_hash_name(
  name: &mut String,
  hash_names: &HashMap<u32, String>,
) {
  let resolved = name
    .strip_prefix("hash_")
    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
    .and_then(|hash| hash_names.get(&hash));
  if let Some(resolved) = resolved {
    *name = resolved.clone();
  }
}

fn ymap_paths(root: &Path) -> Vec<PathBuf> {
  walkdir::WalkDir::new(root)
    .sort_by_file_name()
    .into_iter()
    .filter_map(Result::ok)
    .map(walkdir::DirEntry::into_path)
    .filter(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("ymap")))
    .collect()
}

fn duplicate_guid_groups(
  file: &str,
  entities: Vec<YmapEntity>,
  include_zero_guid: bool,
) -> Vec<DuplicateGuidGroup> {
  let mut order = Vec::new();
  let mut by_guid = HashMap::<u32, Vec<(usize, YmapEntity)>>::new();
  for (index, entity) in entities.into_iter().enumerate() {
    if entity.guid == 0 && !include_zero_guid {
      continue;
    }
    let group = by_guid.entry(entity.guid).or_default();
    if group.is_empty() {
      order.push(entity.guid);
    }
    group.push((index, entity));
  }
  order
    .into_iter()
    .filter_map(|guid| {
      let entities = by_guid.remove(&guid)?;
      (entities.len() > 1).then(|| DuplicateGuidGroup {
        file: file.to_owned(),
        guid,
        count: entities.len(),
        positions: entities
          .iter()
          .map(|(_, entity)| {
            format!("{}, {}, {}", entity.position.x, entity.position.y, entity.position.z)
          })
          .collect(),
        entities,
      })
    })
    .collect()
}
