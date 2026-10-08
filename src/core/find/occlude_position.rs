use super::{FindEntity, Result, aabb_intersects_radius, file_from_position, read_input};
use crate::core::format::{
  gamefile::{
    meta_resource::MetaResource, meta_xml::ymap_to_model_with_entities_from_meta,
    resource_file::Rsc7Resource,
  },
  ymap::model::{YmapBoxOccluder, YmapOccludeModel},
};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use serde::Serialize;
use std::{collections::HashMap, time::Duration};

#[derive(Serialize)]
#[serde(rename = "OccludePositionSearch")]
struct SearchResult {
  #[serde(rename = "@position")]
  position: String,
  #[serde(rename = "@radius")]
  radius: f64,
  #[serde(rename = "File")]
  files: Vec<FileResult>,
}

#[derive(Serialize)]
struct FileResult {
  #[serde(rename = "@name")]
  name: String,
  #[serde(rename = "@stage")]
  stage: &'static str,
  #[serde(rename = "@path")]
  path: String,
  #[serde(rename = "@resource", skip_serializing_if = "Option::is_none")]
  resource: Option<String>,
  #[serde(rename = "@originalPath", skip_serializing_if = "Option::is_none")]
  original_path: Option<String>,
  #[serde(rename = "@version", skip_serializing_if = "Option::is_none")]
  version: Option<String>,
  #[serde(rename = "Item")]
  items: Vec<OccluderItem>,
}

#[derive(Serialize)]
struct OccluderItem {
  #[serde(rename = "@type")]
  kind: &'static str,
  #[serde(rename = "@index")]
  index: usize,
  #[serde(rename = "@distance")]
  distance: f64,
  #[serde(rename = "BoxOccluder", skip_serializing_if = "Option::is_none")]
  box_occluder: Option<YmapBoxOccluder>,
  #[serde(rename = "OccludeModel", skip_serializing_if = "Option::is_none")]
  occlude_model: Option<YmapOccludeModel>,
}

pub(super) fn run(
  search: &FindEntity,
  position: [f64; 3],
  radius: f64,
) -> Result<String> {
  let progress = ProgressBar::with_draw_target(Some(0), ProgressDrawTarget::stderr());
  progress.set_style(ProgressStyle::with_template("{spinner:.green} {msg}")?);
  progress.enable_steady_tick(Duration::from_millis(100));
  progress.set_message("Collecting YMAP occluder inputs");
  let candidates = file_from_position::collect_candidates(search, &progress)?;
  let candidates =
    candidates.into_iter().filter(|candidate| candidate.extension == "ymap").collect::<Vec<_>>();
  progress.set_length(candidates.len() as u64);
  progress.set_style(
    ProgressStyle::with_template(
      "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
    )?
    .progress_chars("##-"),
  );
  let mut files = Vec::new();
  let mut skipped = 0usize;
  for candidate in candidates {
    progress.set_message(candidate.path.display().to_string());
    match read_occluders(&candidate, position, radius) {
      Ok(Some(items)) => files.push(FileResult {
        name: candidate.name,
        stage: candidate.stage,
        path: candidate.path.to_string_lossy().replace('\\', "/"),
        resource: candidate.resource,
        original_path: candidate.original_path,
        version: candidate.version,
        items,
      }),
      Ok(None) => {}
      Err(error) if candidate.strict => return Err(error),
      Err(error) => {
        skipped += 1;
        progress.println(format!("Warning: skipped {}: {error}", candidate.path.display()));
      }
    }
    progress.inc(1);
  }
  let checked = progress.position();
  progress.finish_and_clear();
  eprintln!(
    "occlude-position: checked {checked} YMAP files, found {} files, skipped {skipped}",
    files.len()
  );
  let result = SearchResult {
    position: format!("{},{},{}", position[0], position[1], position[2]),
    radius,
    files,
  };
  let mut xml = String::new();
  let mut serializer = quick_xml::se::Serializer::new(&mut xml);
  serializer.indent(' ', 2);
  result.serialize(serializer)?;
  Ok(xml)
}

fn read_occluders(
  candidate: &file_from_position::Candidate,
  position: [f64; 3],
  radius: f64,
) -> Result<Option<Vec<OccluderItem>>> {
  let bytes = read_input(&candidate.path, candidate.stage, candidate.input.as_ref())?;
  let resource = Rsc7Resource::decode(&bytes)?;
  let meta = MetaResource::parse(&resource)?;
  if let Some((minimum, maximum)) = file_from_position::ymap_extents(&meta)?
    && !aabb_intersects_radius(position, radius, minimum, maximum).unwrap_or(true)
  {
    return Ok(None);
  }
  let (model, _) = ymap_to_model_with_entities_from_meta(&meta, &HashMap::new())?;
  let items = collect_occluders(&model.box_occluders, &model.occlude_models, position, radius);
  Ok((!items.is_empty()).then_some(items))
}

fn collect_occluders(
  boxes: &[YmapBoxOccluder],
  models: &[YmapOccludeModel],
  position: [f64; 3],
  radius: f64,
) -> Vec<OccluderItem> {
  let mut items = Vec::new();
  for (index, occluder) in boxes.iter().enumerate() {
    let center = [
      f64::from(occluder.i_center_x),
      f64::from(occluder.i_center_y),
      f64::from(occluder.i_center_z),
    ];
    let distance = point_distance(position, center);
    if distance <= radius {
      items.push(OccluderItem {
        kind: "boxOccluder",
        index,
        distance,
        box_occluder: Some(occluder.clone()),
        occlude_model: None,
      });
    }
  }
  for (index, model) in models.iter().enumerate() {
    let minimum = [f64::from(model.bmin.x), f64::from(model.bmin.y), f64::from(model.bmin.z)];
    let maximum = [f64::from(model.bmax.x), f64::from(model.bmax.y), f64::from(model.bmax.z)];
    let Some(distance) = point_aabb_distance(position, minimum, maximum) else {
      continue;
    };
    if distance <= radius {
      items.push(OccluderItem {
        kind: "occludeModel",
        index,
        distance,
        box_occluder: None,
        occlude_model: Some(model.clone()),
      });
    }
  }
  items
}

fn point_distance(
  first: [f64; 3],
  second: [f64; 3],
) -> f64 {
  (first[0] - second[0]).hypot(first[1] - second[1]).hypot(first[2] - second[2])
}

fn point_aabb_distance(
  position: [f64; 3],
  minimum: [f64; 3],
  maximum: [f64; 3],
) -> Option<f64> {
  if (0..3).any(|axis| {
    !minimum[axis].is_finite() || !maximum[axis].is_finite() || minimum[axis] > maximum[axis]
  }) {
    return None;
  }
  let nearest = std::array::from_fn(|axis| position[axis].clamp(minimum[axis], maximum[axis]));
  Some(point_distance(position, nearest))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::common::{position::Position, triangle::Triangle};

  #[test]
  fn occluder_search_matches_centers_and_aabb_radius_boundary() {
    let boxes = [YmapBoxOccluder {
      i_center_x: 3,
      i_center_y: 4,
      i_center_z: 0,
      i_cos_z: 0,
      i_length: 2,
      i_width: 2,
      i_height: 2,
      i_sin_z: 0,
    }];
    let models = [YmapOccludeModel {
      bmin: Position {
        x: 5.0,
        y: -1.0,
        z: -1.0,
      },
      bmax: Position {
        x: 7.0,
        y: 1.0,
        z: 1.0,
      },
      triangles: vec![Triangle {
        corner_1: Position::default(),
        corner_2: Position::default(),
        corner_3: Position::default(),
      }],
      flags: 1,
    }];
    let items = collect_occluders(&boxes, &models, [0.0, 0.0, 0.0], 5.0);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].kind, "boxOccluder");
    assert_eq!(items[0].distance, 5.0);
    assert_eq!(items[1].kind, "occludeModel");
    assert_eq!(items[1].distance, 5.0);
    let box_xml = quick_xml::se::to_string(&items[0]).unwrap();
    let model_xml = quick_xml::se::to_string(&items[1]).unwrap();
    assert!(box_xml.contains("BoxOccluder") && box_xml.contains("i_center_x"));
    assert!(model_xml.contains("OccludeModel") && model_xml.contains("bmin"));
    assert_eq!(collect_occluders(&boxes, &models, [0.0; 3], 4.99).len(), 0);
  }
}
