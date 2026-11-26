//! Hash-name and YMAP entity archetype extraction for raw vanilla generation.

use crate::platform::fs;
use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Component, Path, PathBuf},
};

use rayon::prelude::*;
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::Result;
use crate::common::json::write_json;
use crate::core::format::gamefile::{
  meta::{MetaResource, jenk_hash},
  rsc7::Rsc7Resource,
};
use crate::core::game_archive::{VanillaCacheManifest, archives::RpfNames};

const ARRAY_INFO_HASH: u32 = 0x100;
#[cfg(test)]
const STRING_BLOCK_HASH: u32 = 0x10;

#[derive(Debug, Serialize)]
struct HashIndex {
  format_version: u32,
  latest_version: String,
  vanilla_manifest_sha256: String,
  names: BTreeMap<String, String>,
}

#[derive(Debug)]
struct ParsedYmapNames {
  names: Vec<(u32, String)>,
  archetypes: BTreeSet<u32>,
}

pub(crate) fn write_hash_index(
  vanilla_dir: &Path,
  manifest: &VanillaCacheManifest,
  rpf_names: &RpfNames,
) -> Result<()> {
  let manifest_bytes = fs::read(vanilla_dir.join("cache_info.json"))?;
  let latest_version = manifest.versions.last().ok_or("Vanilla archive has no stages")?.id.clone();
  let files = manifest.resolve_version(&latest_version)?;
  let revision = hex::encode(Sha256::digest(&manifest_bytes));

  let ymaps = files
    .iter()
    .filter(|(file_name, _)| file_name.to_ascii_lowercase().ends_with(".ymap"))
    .collect::<Vec<_>>();
  tracing::info!("Extracting hash names from {} vanilla YMAPs", ymaps.len());
  // Each YMAP is read and parsed whole, so they are read in parallel and merged in order.
  let parsed = ymaps
    .par_iter()
    .map(|(_, file)| -> std::result::Result<Option<ParsedYmapNames>, String> {
      let path =
        checked_artifact_path(vanilla_dir, &file.object).map_err(|error| error.to_string())?;
      let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
      if !bytes.starts_with(b"RSC7") {
        tracing::debug!("Skipping non-RSC7 YMAP hash extraction: {}", path.display());
        return Ok(None);
      }
      let parsed = Rsc7Resource::decode(&bytes).and_then(|resource| {
        let meta = MetaResource::parse(&resource)?;
        parse_ymap_names(&meta)
      });
      match parsed {
        Ok(parsed) => Ok(Some(parsed)),
        Err(error) => {
          tracing::warn!("Could not extract YMAP names from {}: {error}", path.display());
          Ok(None)
        }
      }
    })
    .collect::<std::result::Result<Vec<_>, String>>()?;
  let mut names = BTreeMap::new();
  let mut archetypes = BTreeSet::new();
  for parsed in parsed.into_iter().flatten() {
    for (hash, name) in parsed.names {
      names.entry(format!("{hash:08X}")).or_insert(name);
    }
    archetypes.extend(parsed.archetypes);
  }
  add_rpf_name_candidates(&mut names, rpf_names.stems.iter().cloned());

  names.extend(entity_archetype_names(&archetypes, &names, &rpf_names.drawables));

  write_json(
    &hash_names_path(vanilla_dir),
    &HashIndex {
      format_version: 1,
      latest_version,
      vanilla_manifest_sha256: revision,
      names,
    },
  )?;
  Ok(())
}

fn parse_ymap_names(meta: &MetaResource) -> std::io::Result<ParsedYmapNames> {
  let mut names: Vec<_> = meta.hash_names().into_iter().collect();
  names.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
  let root_index = usize::try_from(meta.root_block_index - 1)
    .map_err(|_| invalid_data("YMAP META root block index is invalid"))?;
  let root = meta
    .data_blocks
    .get(root_index)
    .ok_or_else(|| invalid_data("YMAP META root block is missing"))?;
  let schema = meta
    .structures
    .iter()
    .find(|schema| schema.name_hash == root.structure_name_hash)
    .ok_or_else(|| invalid_data("YMAP META root schema is missing"))?;
  let Some(entities_field) =
    schema.entries.iter().find(|field| field.name_hash == jenk_hash("entities"))
  else {
    return Ok(ParsedYmapNames {
      names,
      archetypes: BTreeSet::new(),
    });
  };
  if entities_field.data_type != 0x52 {
    return Err(invalid_data("YMAP entities field is not an array"));
  }
  let array_info = schema
    .entries
    .iter()
    .find(|field| field.name_hash == ARRAY_INFO_HASH)
    .ok_or_else(|| invalid_data("YMAP entities array metadata is missing"))?;
  let descriptor = checked_slice(
    &root.data,
    entities_field.data_offset as usize,
    16,
    "YMAP entities descriptor is truncated",
  )?;
  let pointer = u64::from_le_bytes(descriptor[..8].try_into().unwrap());
  let count = u16::from_le_bytes(descriptor[8..10].try_into().unwrap()) as usize;
  let block_id = (pointer & 0xfff) as usize;
  let mut archetypes = BTreeSet::new();
  if count == 0 || block_id == 0 {
    return Ok(ParsedYmapNames {
      names,
      archetypes,
    });
  }
  let mut offset = ((pointer >> 12) & 0xfffff) as usize;
  match array_info.data_type {
    0x05 => {
      let entity_schema = meta
        .structures
        .iter()
        .find(|schema| schema.name_hash == array_info.reference_key)
        .ok_or_else(|| invalid_data("YMAP entity schema is missing"))?;
      let archetype_field = entity_schema
        .entries
        .iter()
        .find(|field| field.name_hash == jenk_hash("archetypeName"))
        .ok_or_else(|| invalid_data("YMAP entity archetypeName field is missing"))?;
      if archetype_field.data_type != 0x4a {
        return Err(invalid_data("YMAP entity archetypeName is not a hash"));
      }
      let mut block_index = block_id - 1;
      for _ in 0..count {
        let block = meta
          .data_blocks
          .get(block_index)
          .ok_or_else(|| invalid_data("YMAP entity array references a missing block"))?;
        let hash_offset = offset
          .checked_add(archetype_field.data_offset as usize)
          .ok_or_else(|| invalid_data("YMAP archetypeName offset overflows"))?;
        let hash_bytes =
          checked_slice(&block.data, hash_offset, 4, "YMAP archetypeName is truncated")?;
        let hash = u32::from_le_bytes(hash_bytes.try_into().unwrap());
        if hash != 0 {
          archetypes.insert(hash);
        }
        offset = offset
          .checked_add(entity_schema.structure_size as usize)
          .ok_or_else(|| invalid_data("YMAP entity array offset overflows"))?;
        if offset >= block.data.len() {
          offset -= block.data.len();
          block_index += 1;
        }
      }
    }
    0x07 => {
      let pointer_block = meta
        .data_blocks
        .get(block_id - 1)
        .ok_or_else(|| invalid_data("YMAP entity pointer array references a missing block"))?;
      for index in 0..count {
        let entry_offset = offset
          .checked_add(
            index
              .checked_mul(8)
              .ok_or_else(|| invalid_data("YMAP entity pointer offset overflows"))?,
          )
          .ok_or_else(|| invalid_data("YMAP entity pointer offset overflows"))?;
        let pointer_bytes =
          checked_slice(&pointer_block.data, entry_offset, 8, "YMAP entity pointer is truncated")?;
        let entity_pointer = u64::from_le_bytes(pointer_bytes.try_into().unwrap());
        let entity_block_id = (entity_pointer & 0xfff) as usize;
        if entity_block_id == 0 {
          continue;
        }
        let entity_offset = ((entity_pointer >> 12) & 0xfffff) as usize;
        let entity_block = meta
          .data_blocks
          .get(entity_block_id - 1)
          .ok_or_else(|| invalid_data("YMAP entity pointer references a missing block"))?;
        if let Some(hash) = read_entity_archetype_hash(
          meta,
          entity_block.structure_name_hash,
          &entity_block.data,
          entity_offset,
        )? {
          archetypes.insert(hash);
        }
      }
    }
    _ => return Err(invalid_data("YMAP entities use an unsupported array type")),
  }
  Ok(ParsedYmapNames {
    names,
    archetypes,
  })
}

fn read_entity_archetype_hash(
  meta: &MetaResource,
  structure_hash: u32,
  data: &[u8],
  offset: usize,
) -> std::io::Result<Option<u32>> {
  let Some(schema) = meta.structures.iter().find(|schema| schema.name_hash == structure_hash)
  else {
    return Err(invalid_data("YMAP entity schema is missing"));
  };
  let Some(field) =
    schema.entries.iter().find(|field| field.name_hash == jenk_hash("archetypeName"))
  else {
    return Ok(None);
  };
  if field.data_type != 0x4a {
    return Err(invalid_data("YMAP entity archetypeName is not a hash"));
  }
  let hash_offset = offset
    .checked_add(field.data_offset as usize)
    .ok_or_else(|| invalid_data("YMAP archetypeName offset overflows"))?;
  let bytes = checked_slice(data, hash_offset, 4, "YMAP archetypeName is truncated")?;
  let hash = u32::from_le_bytes(bytes.try_into().unwrap());
  Ok((hash != 0).then_some(hash))
}

fn checked_artifact_path(
  vanilla_dir: &Path,
  object: &str,
) -> Result<PathBuf> {
  let relative = Path::new(object);
  if object.is_empty()
    || object.contains(['\\', ':'])
    || relative.components().any(|component| !matches!(component, Component::Normal(_)))
  {
    return Err(format!("Invalid vanilla artifact path: {object}").into());
  }
  let path = vanilla_dir.join(relative);
  if !fs::is_file(&path) {
    return Err(format!("Vanilla artifact is missing: {}", path.display()).into());
  }
  Ok(path)
}

fn checked_slice<'a>(
  bytes: &'a [u8],
  offset: usize,
  length: usize,
  message: &str,
) -> std::io::Result<&'a [u8]> {
  let end = offset.checked_add(length).ok_or_else(|| invalid_data(message))?;
  bytes.get(offset..end).ok_or_else(|| invalid_data(message))
}

fn invalid_data(message: &str) -> std::io::Error {
  std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}

fn add_rpf_name_candidates(
  names: &mut BTreeMap<String, String>,
  candidates: impl IntoIterator<Item = String>,
) {
  let mut candidates: Vec<_> = candidates.into_iter().collect();
  candidates.sort();
  candidates.dedup();
  for name in candidates {
    let hash = jenk_hash(&name);
    names.entry(format!("{hash:08X}")).or_insert(name);
  }
}

/// Names every entity archetype, deriving LOD names from drawable files for hashes no
/// other name matches and keeping the rest as `hash_XXXXXXXX`.
fn entity_archetype_names(
  archetypes: &BTreeSet<u32>,
  names: &BTreeMap<String, String>,
  drawables: &BTreeSet<String>,
) -> BTreeMap<String, String> {
  let unresolved = archetypes
    .iter()
    .copied()
    .filter(|hash| !names.contains_key(&format!("{hash:08X}")))
    .collect::<BTreeSet<_>>();
  let mut derived = BTreeMap::new();
  for drawable in drawables {
    for name in lod_name_candidates(drawable) {
      let hash = jenk_hash(&name);
      if unresolved.contains(&hash) {
        derived.entry(hash).or_insert(name);
      }
    }
  }
  archetypes
    .iter()
    .map(|hash| {
      let key = format!("{hash:08X}");
      let name = names
        .get(&key)
        .or_else(|| derived.get(hash))
        .cloned()
        .unwrap_or_else(|| format!("hash_{key}"));
      (key, name)
    })
    .collect()
}

/// LOD archetype names implied by a lowercased `.ydr` or `.ydd` file name.
///
/// LOD archetypes are often stored in drawable dictionaries under hashed names, so no
/// file or META string spells them out. These are the names CodeWalker's extended hash
/// index derives from the same files.
pub(crate) fn lod_name_candidates(file_name: &str) -> Vec<String> {
  let mut candidates = Vec::new();
  if let Some(stem) = file_name.strip_suffix(".ydr") {
    candidates.extend(["_lod", "_loda", "_lodb"].map(|suffix| format!("{stem}{suffix}")));
  } else if let Some(stem) = file_name.strip_suffix(".ydd") {
    if let Some(base) = stem.strip_suffix("_children") {
      candidates.push(base.to_owned());
      candidates.extend(["_lod", "_loda", "_lodb"].map(|suffix| format!("{base}{suffix}")));
    }
    if let Some((group, _)) = stem.rsplit_once('_')
      && let Some((base, _)) = group.rsplit_once('_')
      && !base.is_empty()
    {
      candidates.push(format!("{base}_lod"));
      candidates.extend((1..=100).map(|index| format!("{base}_{index:02}_lod")));
    }
  }
  candidates
}

/// The vanilla archive's index of the names its hashes spell, `hash_names.json`.
pub fn hash_names_path(vanilla_dir: &Path) -> PathBuf {
  vanilla_dir.join("hash_names.json")
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::format::gamefile::meta::{MetaDataBlock, MetaStructureEntry, MetaStructureInfo};

  #[test]
  fn extracts_entity_archetype_hashes_without_guids() {
    let root_hash = jenk_hash("CMapData");
    let entity_hash = jenk_hash("CEntityDef");
    let prop_hash = jenk_hash("prop_toolchest_01");
    let meta = MetaResource {
      root_block_index: 1,
      structures: vec![
        MetaStructureInfo {
          name_hash: root_hash,
          structure_key: 0,
          unknown_8: 0,
          unknown_12: 0,
          unknown_28: 0,
          structure_size: 16,
          entries: vec![
            MetaStructureEntry {
              name_hash: ARRAY_INFO_HASH,
              data_offset: 0,
              data_type: 0x07,
              unknown: 0,
              reference_type_index: -1,
              reference_key: 0,
            },
            MetaStructureEntry {
              name_hash: jenk_hash("entities"),
              data_offset: 0,
              data_type: 0x52,
              unknown: 0,
              reference_type_index: -1,
              reference_key: 0,
            },
          ],
        },
        MetaStructureInfo {
          name_hash: entity_hash,
          structure_key: 0,
          unknown_8: 0,
          unknown_12: 0,
          unknown_28: 0,
          structure_size: 8,
          entries: vec![MetaStructureEntry {
            name_hash: jenk_hash("archetypeName"),
            data_offset: 0,
            data_type: 0x4a,
            unknown: 0,
            reference_type_index: -1,
            reference_key: 0,
          }],
        },
      ],
      enums: vec![],
      data_blocks: vec![
        MetaDataBlock {
          structure_name_hash: root_hash,
          data: [2u64.to_le_bytes().as_slice(), 1u16.to_le_bytes().as_slice(), &[0; 6]].concat(),
        },
        MetaDataBlock {
          structure_name_hash: 0x07,
          data: 3u64.to_le_bytes().to_vec(),
        },
        MetaDataBlock {
          structure_name_hash: entity_hash,
          data: [prop_hash.to_le_bytes().as_slice(), &[0; 4]].concat(),
        },
        MetaDataBlock {
          structure_name_hash: STRING_BLOCK_HASH,
          data: b"prop_toolchest_01\0".to_vec(),
        },
      ],
      name: None,
    };
    let parsed = parse_ymap_names(&meta).unwrap();
    assert!(parsed.archetypes.contains(&prop_hash));
    assert_eq!(parsed.names, [(prop_hash, "prop_toolchest_01".into())]);
    assert!(!parsed.names.iter().any(|(_, name)| name == "GUID"));
  }

  #[test]
  fn rpf_entry_names_resolve_archetype_hashes_missing_from_meta_strings() {
    let prop = "prop_toolchest_01";
    let hash = jenk_hash(prop);
    let mut names = BTreeMap::new();
    add_rpf_name_candidates(&mut names, [prop.to_owned()]);
    names.extend(entity_archetype_names(&BTreeSet::from([hash]), &names, &BTreeSet::new()));
    let index = HashIndex {
      format_version: 1,
      latest_version: "latest".into(),
      vanilla_manifest_sha256: "revision".into(),
      names,
    };
    let json = serde_json::to_value(index).unwrap();
    assert_eq!(json["names"][format!("{hash:08X}")], prop);
    assert!(json.get("entity_archetypes").is_none());
  }

  #[test]
  fn unresolved_archetype_hash_is_retained_in_names_as_hash_text() {
    let hash = 0x0001_D65D;
    let names = BTreeMap::new();
    let all_names = entity_archetype_names(&BTreeSet::from([hash]), &names, &BTreeSet::new());
    assert_eq!(all_names["0001D65D"], "hash_0001D65D");
  }

  #[test]
  fn drawable_file_names_resolve_lod_archetype_hashes() {
    let slod = jenk_hash("sm_22_pnwstepslod");
    let lod = jenk_hash("sm_22_office_lod");
    let numbered = jenk_hash("hei_dt1_07_lod");
    let drawables = BTreeSet::from([
      "sm_22_pnwstepslod_children.ydd".to_owned(),
      "sm_22_office.ydr".to_owned(),
      "hei_dt1_02_children.ydd".to_owned(),
    ]);
    let names =
      entity_archetype_names(&BTreeSet::from([slod, lod, numbered]), &BTreeMap::new(), &drawables);
    assert_eq!(names[&format!("{slod:08X}")], "sm_22_pnwstepslod");
    assert_eq!(names[&format!("{lod:08X}")], "sm_22_office_lod");
    assert_eq!(names[&format!("{numbered:08X}")], "hei_dt1_07_lod");
  }
}
