//! Derivation tests for every field of [`YmapDiffFields`], through `Ymap`'s [`Diff`].
//!
//! Each test changes one kind of data and checks the serialized diff, so it also covers that
//! unchanged fields are omitted from the JSON.

mod apply;
mod box_occluders;
mod car_generators;
mod context;
mod distant_lod_lights;
mod document;
mod entities;
mod field;
mod instanced_data;
mod lod_lights;
mod merge;
mod metadata;
mod occlude_models;
mod physics_dictionaries;
mod time_cycle_modifiers;

use serde_json::Value;
use struct_diff::Diff;

use super::YmapDiffFields;
use crate::{
  common::position::Position,
  core::{
    config::matching::MatchTolerances,
    format::ymap::{
      compare,
      model::{Ymap, YmapDistantLodLights, YmapEntity, YmapInstancedData},
      xml::XmlYmap,
    },
  },
};

fn sample() -> Ymap {
  let xml: XmlYmap = quick_xml::de::from_str(include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/sample/parent_refs/vanilla_parent.ymap.xml"
  )))
  .unwrap();
  xml.into()
}

/// A YMAP with every array cleared, so a test fills in only what it compares.
fn base() -> Ymap {
  Ymap {
    entities: Vec::new(),
    box_occluders: Vec::new(),
    occlude_models: Vec::new(),
    lod_lights: Vec::new(),
    distant_lod_lights: YmapDistantLodLights::default(),
    container_lods: Vec::new(),
    physics_dictionaries: Vec::new(),
    instanced_data: YmapInstancedData::default(),
    time_cycle_modifiers: Vec::new(),
    car_generators: Vec::new(),
    ..sample()
  }
}

/// An entity from the sample with the given GUID.
fn entity(guid: u32) -> YmapEntity {
  YmapEntity {
    guid,
    ..sample().entities[0].clone()
  }
}

fn at(
  x: f32,
  y: f32,
  z: f32,
) -> Position {
  Position {
    x,
    y,
    z,
  }
}

fn diff_with(
  vanilla: &Ymap,
  modded: &Ymap,
  tolerances: &MatchTolerances,
) -> YmapDiffFields {
  vanilla.diff(modded, tolerances).unwrap_or_default()
}

fn diff(
  vanilla: &Ymap,
  modded: &Ymap,
) -> YmapDiffFields {
  diff_with(vanilla, modded, &MatchTolerances::default())
}

/// The diff as it is written to `.diff.json`.
fn json(
  vanilla: &Ymap,
  modded: &Ymap,
) -> Value {
  serde_json::to_value(diff(vanilla, modded)).unwrap()
}
