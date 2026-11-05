//! Entity searches by GUID or position, rendered as CodeWalker-style XML.

use super::{
  EntityQuery, FindEntity, Result, file_from_position,
  inputs::{Input, read_input, search_inputs},
  query::aabb_intersects_radius,
};
use crate::core::format::{
  xml::{XmlValueAttr, position::XmlPositionAttr, rotation::XmlRotation},
  ymap::{binary::ymap_to_model, model::YmapEntity},
};
use serde::Serialize;
use std::{collections::HashMap, path::Path};

#[derive(Serialize)]
#[serde(rename = "EntityGuidSearch")]
pub(super) struct SearchResult {
  #[serde(rename = "@guid", skip_serializing_if = "Option::is_none")]
  pub(super) guid: Option<u32>,
  #[serde(rename = "@position", skip_serializing_if = "Option::is_none")]
  pub(super) position: Option<String>,
  #[serde(rename = "@radius", skip_serializing_if = "Option::is_none")]
  pub(super) radius: Option<f64>,
  #[serde(rename = "File")]
  pub(super) files: Vec<FileResult>,
}
#[derive(Serialize)]
pub(super) struct FileResult {
  #[serde(rename = "@name")]
  pub(super) name: String,
  #[serde(rename = "Data")]
  pub(super) data: Vec<EntityData>,
}
#[derive(Serialize)]
pub(super) struct EntityData {
  #[serde(rename = "@stage")]
  pub(super) stage: &'static str,
  #[serde(rename = "@path", skip_serializing_if = "Option::is_none")]
  pub(super) path: Option<String>,
  #[serde(rename = "@resource", skip_serializing_if = "Option::is_none")]
  pub(super) resource: Option<String>,
  #[serde(rename = "@originalPath", skip_serializing_if = "Option::is_none")]
  pub(super) original_path: Option<String>,
  #[serde(rename = "@found")]
  pub(super) found: bool,
  #[serde(rename = "Item")]
  pub(super) entities: Vec<FoundEntity>,
}

/// An entity as CodeWalker writes `CEntityDef` items, without extensions.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FoundEntity {
  #[serde(rename = "@type")]
  pub(super) entity_type: String,
  pub(super) archetype_name: String,
  pub(super) flags: XmlValueAttr<u32>,
  pub(super) guid: XmlValueAttr<u32>,
  pub(super) position: XmlPositionAttr,
  pub(super) rotation: XmlRotation,
  pub(super) scale_x_y: XmlValueAttr<f32>,
  pub(super) scale_z: XmlValueAttr<f32>,
  pub(super) parent_index: XmlValueAttr<i32>,
  pub(super) lod_dist: XmlValueAttr<f32>,
  pub(super) child_lod_dist: XmlValueAttr<f32>,
  pub(super) lod_level: String,
  pub(super) num_children: XmlValueAttr<u32>,
  pub(super) priority_level: String,
  pub(super) extensions: Option<()>,
  pub(super) ambient_occlusion_multiplier: XmlValueAttr<u8>,
  pub(super) artificial_ambient_occlusion: XmlValueAttr<u8>,
  pub(super) tint_value: XmlValueAttr<u32>,
}

impl From<YmapEntity> for FoundEntity {
  fn from(entity: YmapEntity) -> Self {
    let value = |value| XmlValueAttr {
      value,
    };
    Self {
      entity_type: entity.entity_type,
      archetype_name: entity.archetype_name,
      flags: value(entity.flags),
      guid: value(entity.guid),
      position: entity.position.into(),
      rotation: entity.rotation.into(),
      scale_x_y: XmlValueAttr {
        value: entity.scale_x_y,
      },
      scale_z: XmlValueAttr {
        value: entity.scale_z,
      },
      parent_index: XmlValueAttr {
        value: entity.parent_index,
      },
      lod_dist: XmlValueAttr {
        value: entity.lod_dist,
      },
      child_lod_dist: XmlValueAttr {
        value: entity.child_lod_dist,
      },
      lod_level: entity.lod_level,
      num_children: value(entity.num_children),
      priority_level: entity.priority_level,
      extensions: None,
      ambient_occlusion_multiplier: XmlValueAttr {
        value: entity.ambient_occlusion_multiplier,
      },
      artificial_ambient_occlusion: XmlValueAttr {
        value: entity.artificial_ambient_occlusion,
      },
      tint_value: value(entity.tint_value),
    }
  }
}

/// Searches the merged YMAPs, and with `--all` the inputs the merge recorded, for entities.
pub(super) fn run(search: &FindEntity) -> Result<String> {
  let (paths, cache, names) = search_inputs(search, "ymap")?;
  let mut files = Vec::new();
  for name in names {
    let mut data = Vec::new();
    if let Some(path) = paths.get(&name) {
      data.push(read_entities(path, &search.query, "merged", None)?);
    } else {
      data.push(EntityData {
        stage: "merged",
        path: None,
        resource: None,
        original_path: None,
        found: false,
        entities: Vec::new(),
      });
    }
    if let Some(cache) = &cache {
      let record = cache
        .files
        .get(&name)
        .ok_or_else(|| format!("No pre-merge provenance recorded for {name}"))?;
      if let Some(vanilla) = &record.vanilla {
        data.push(read_entities(&vanilla.path, &search.query, "vanilla", Some(vanilla))?);
      }
      for source in &record.merge_sources {
        data.push(read_entities(&source.path, &search.query, "source", Some(source))?);
      }
    }
    if data.iter().any(|data| data.found) {
      files.push(FileResult {
        name,
        data,
      });
    }
  }
  let result = search_result(&search.query, files);
  let mut xml = String::new();
  let root = match search.query {
    EntityQuery::Guid(_) => "EntityGuidSearch",
    EntityQuery::Position {
      ..
    } => "EntityPositionSearch",
    EntityQuery::YbnPosition {
      ..
    } => unreachable!(),
    EntityQuery::FileFromPosition {
      ..
    } => unreachable!(),
    EntityQuery::OccludePosition {
      ..
    } => unreachable!(),
  };
  let mut serializer = quick_xml::se::Serializer::with_root(&mut xml, Some(root))?;
  serializer.indent(' ', 2);
  result.serialize(serializer)?;
  Ok(xml)
}

pub(super) fn search_result(
  query: &EntityQuery,
  files: Vec<FileResult>,
) -> SearchResult {
  match query {
    EntityQuery::Guid(guid) => SearchResult {
      guid: Some(*guid),
      position: None,
      radius: None,
      files,
    },
    EntityQuery::Position {
      position,
      radius,
    } => SearchResult {
      guid: None,
      position: Some(format!("{},{},{}", position[0], position[1], position[2])),
      radius: Some(*radius),
      files,
    },
    EntityQuery::YbnPosition {
      ..
    }
    | EntityQuery::FileFromPosition {
      ..
    }
    | EntityQuery::OccludePosition {
      ..
    } => unreachable!(),
  }
}

pub(super) fn read_entities(
  path: &Path,
  query: &EntityQuery,
  stage: &'static str,
  input: Option<&Input>,
) -> Result<EntityData> {
  let model = ymap_to_model(&read_input(path, stage, input)?, &HashMap::new())?;
  if let EntityQuery::Position {
    position,
    radius,
  } = query
  {
    let (minimum, maximum) = file_from_position::ymap_extents(&model);
    if !aabb_intersects_radius(position.map(|value| value as f32 as f64), *radius, minimum, maximum)
      .unwrap_or(true)
    {
      return Ok(EntityData {
        stage,
        path: Some(path.to_string_lossy().replace('\\', "/")),
        resource: input.and_then(|input| input.resource.clone()),
        original_path: input
          .and_then(|input| input.original_path.as_ref())
          .map(|path| path.to_string_lossy().replace('\\', "/")),
        found: false,
        entities: Vec::new(),
      });
    }
  }
  Ok(entity_data(path, query, stage, input, model.entities))
}

pub(super) fn entity_data(
  path: &Path,
  query: &EntityQuery,
  stage: &'static str,
  input: Option<&Input>,
  entities: Vec<YmapEntity>,
) -> EntityData {
  let entities = entities
    .into_iter()
    .filter(|entity| query.contains(entity))
    .map(FoundEntity::from)
    .collect::<Vec<_>>();
  EntityData {
    stage,
    path: Some(path.to_string_lossy().replace('\\', "/")),
    resource: input.and_then(|input| input.resource.clone()),
    original_path: input
      .and_then(|input| input.original_path.as_ref())
      .map(|path| path.to_string_lossy().replace('\\', "/")),
    found: !entities.is_empty(),
    entities,
  }
}
