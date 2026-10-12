use std::{collections::HashMap, io};

use crate::core::format::gamefile::{
  meta::{
    MetaResource,
    xml::{meta_to_value, meta_to_xml},
  },
  resource::Resource,
};

pub(super) fn invalid_data(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Converts a binary RSC7 or PSO YMAP into CodeWalker-style XML using its embedded schema.
pub fn ymap_to_xml(
  bytes: &[u8],
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<String> {
  match Resource::decode(bytes)? {
    Resource::Pso(resource) => resource.to_xml(shared_hash_names),
    Resource::Rsc7(resource) => {
      let meta = MetaResource::parse(&resource)?;
      meta_to_xml(&meta, shared_hash_names)
    }
  }
}

/// Converts a binary RSC7 or PSO YMAP into its typed model.
///
/// RSC7 uses direct model decoding; PSO uses its embedded-schema XML adapter.
pub fn ymap_to_model(
  bytes: &[u8],
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<crate::core::format::ymap::model::Ymap> {
  ymap_to_model_with_entities(bytes, shared_hash_names).map(|(model, _)| model)
}

pub(crate) fn ymap_to_model_with_entities(
  bytes: &[u8],
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<(
  crate::core::format::ymap::model::Ymap,
  Vec<crate::core::format::ymap::model::YmapEntity>,
)> {
  ymap_to_model_with_entities_inner(bytes, shared_hash_names, false)
}

/// Reads a light-editing snapshot without silently truncating malformed SOA arrays.
pub(crate) fn ymap_to_light_model_with_entities(
  bytes: &[u8],
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<(
  crate::core::format::ymap::model::Ymap,
  Vec<crate::core::format::ymap::model::YmapEntity>,
)> {
  ymap_to_model_with_entities_inner(bytes, shared_hash_names, true)
}

fn ymap_to_model_with_entities_inner(
  bytes: &[u8],
  shared_hash_names: &HashMap<u32, String>,
  strict_lights: bool,
) -> io::Result<(
  crate::core::format::ymap::model::Ymap,
  Vec<crate::core::format::ymap::model::YmapEntity>,
)> {
  match Resource::decode(bytes)? {
    Resource::Pso(resource) => {
      let xml = resource.to_xml(shared_hash_names)?;
      let parsed =
        quick_xml::de::from_str(&xml).map_err(|error| invalid_data(&error.to_string()))?;
      if strict_lights {
        super::light_validation::validate(&parsed)?;
      }
      Ok(ymap_from_parsed(parsed))
    }
    Resource::Rsc7(resource) => {
      let meta = MetaResource::parse(&resource)?;
      ymap_from_meta_inner(&meta, shared_hash_names, strict_lights)
    }
  }
}

pub(crate) fn ymap_to_model_with_entities_from_meta(
  meta: &MetaResource,
  shared_hash_names: &HashMap<u32, String>,
) -> io::Result<(
  crate::core::format::ymap::model::Ymap,
  Vec<crate::core::format::ymap::model::YmapEntity>,
)> {
  ymap_from_meta_inner(meta, shared_hash_names, false)
}

fn ymap_from_meta_inner(
  meta: &MetaResource,
  shared_hash_names: &HashMap<u32, String>,
  strict_lights: bool,
) -> io::Result<(
  crate::core::format::ymap::model::Ymap,
  Vec<crate::core::format::ymap::model::YmapEntity>,
)> {
  let value = meta_to_value(meta, shared_hash_names)?;
  let parsed: crate::core::format::ymap::xml::XmlYmap =
    serde_json::from_value(value).map_err(|error| invalid_data(&error.to_string()))?;
  if strict_lights {
    super::light_validation::validate(&parsed)?;
  }
  Ok(ymap_from_parsed(parsed))
}

fn ymap_from_parsed(
  mut parsed: crate::core::format::ymap::xml::XmlYmap
) -> (crate::core::format::ymap::model::Ymap, Vec<crate::core::format::ymap::model::YmapEntity>) {
  let entities = std::mem::take(&mut parsed.entities.items)
    .into_iter()
    .map(crate::core::format::ymap::model::YmapEntity::from)
    .collect::<Vec<_>>();
  let mut model: crate::core::format::ymap::model::Ymap = parsed.into();
  model.entity_map = entities.iter().map(|entity| (entity.guid, entity.clone())).collect();
  (model, entities)
}
