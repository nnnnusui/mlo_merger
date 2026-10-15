use std::{collections::HashMap, path::Path};

use crate::common::{position::Position, rotation::Rotation};
use crate::core::format::{
  gamefile::pso::PsoResource,
  ymap::model::{Ymap, YmapBlock, YmapDistantLodLights, YmapEntity, YmapInstancedData},
};

fn read(name: &str) -> Ymap {
  let path = Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("asset/vanilla-cache/latest/ymap")
    .join(format!("{name}.ymap"));
  let resource = PsoResource::parse(&std::fs::read(path).unwrap()).unwrap();
  super::pso_to_ymap(&resource, &HashMap::new()).unwrap()
}

fn position(
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

/// A YMAP with the given header and block and no content sections.
fn header(
  (name, parent, flags, content_flags): (&str, &str, u32, u32),
  extents: [Position; 4],
  block: YmapBlock,
) -> Ymap {
  let [streaming_extents_min, streaming_extents_max, entities_extents_min, entities_extents_max] =
    extents;
  Ymap {
    name: name.into(),
    parent: parent.into(),
    flags,
    content_flags,
    streaming_extents_min,
    streaming_extents_max,
    entities_extents_min,
    entities_extents_max,
    entities: Vec::new(),
    box_occluders: Vec::new(),
    occlude_models: Vec::new(),
    lod_lights: Vec::new(),
    distant_lod_lights: YmapDistantLodLights::default(),
    block,
    container_lods: Vec::new(),
    physics_dictionaries: Vec::new(),
    instanced_data: YmapInstancedData::default(),
    time_cycle_modifiers: Vec::new(),
    car_generators: Vec::new(),
  }
}

fn block(
  name: &str,
  exported_by: &str,
  time: &str,
) -> YmapBlock {
  YmapBlock {
    version: 0,
    flags: 0,
    name: name.into(),
    exported_by: exported_by.into(),
    owner: String::new(),
    time: time.into(),
  }
}

/// An unrotated, unscaled `CEntityDef` with full ambient occlusion and no tint.
fn entity(
  (archetype_name, flags, guid): (&str, u32, u32),
  position: Position,
  (parent_index, lod_dist, child_lod_dist): (i32, f32, f32),
  (lod_level, num_children): (&str, u32),
) -> YmapEntity {
  YmapEntity {
    entity_type: "CEntityDef".into(),
    archetype_name: archetype_name.into(),
    flags,
    guid,
    position,
    rotation: Rotation {
      x: 0.0,
      y: 0.0,
      z: 0.0,
      w: 1.0,
    },
    scale_x_y: 1.0,
    scale_z: 1.0,
    parent_index,
    lod_dist,
    child_lod_dist,
    lod_level: lod_level.into(),
    num_children,
    priority_level: "PRI_REQUIRED".into(),
    ambient_occlusion_multiplier: 255,
    artificial_ambient_occlusion: 255,
    tint_value: 0,
  }
}

/// Splits off the entities so headers can be compared as whole structs.
fn split(mut model: Ymap) -> (Ymap, Vec<YmapEntity>) {
  let entities = std::mem::take(&mut model.entities);
  (model, entities)
}

#[test]
fn decodes_cs1_railwyc_header_and_signed_guid_entities() {
  let (model, entities) = split(read("cs1_railwyc"));
  assert_eq!(
    model,
    header(
      ("cs1_railwyc", "", 2, 18),
      [
        position(-2405.707, 3494.25, -1936.4152),
        position(1594.293, 7600.2705, 2063.585),
        position(-550.5755, 4312.0464, 35.334885),
        position(-276.82526, 6002.108, 90.54872),
      ],
      block("cs1_railwyc", "thomas.wicks", "31 May 2012 17:42"),
    )
  );
  assert_eq!(entities.len(), 12);
  // Stored as the signed value -382852075; the model keeps its unsigned bit pattern.
  assert_eq!(
    entities[0],
    entity(
      ("cs1_railwyc_2_slod1", 1572888, 3912115221),
      position(-512.8971, 4968.1025, 88.192245),
      (-1, 1000.0, 500.0),
      ("LODTYPES_DEPTH_SLOD1", 3),
    )
  );
  assert_eq!(
    entities[1],
    entity(
      ("cs1_railwyc_1_slod1", 1572888, 1013253189),
      position(-405.70703, 5600.2705, 63.584854),
      (-1, 2000.0, 500.0),
      ("LODTYPES_DEPTH_SLOD1", 4),
    )
  );
  assert_eq!(
    entities[11],
    entity(
      ("cs1_railwyc_trk01_lod", 1572864, 4280708248),
      position(-384.43756, 5838.2803, 50.320374),
      (1, 500.0, 299.0),
      ("LODTYPES_DEPTH_LOD", 1),
    )
  );
}

#[test]
fn decodes_cs1_railwyc_long_0_child_entities() {
  let (model, entities) = split(read("cs1_railwyc_long_0"));
  assert_eq!(
    model,
    header(
      ("cs1_railwyc_long_0", "cs1_railwyc", 0, 1),
      [
        position(-843.0997, 4132.5156, -260.59995),
        position(-7.1800537, 6269.826, 388.41455),
        position(-550.5755, 4312.0464, 35.322903),
        position(-276.03986, 6002.666, 90.72295),
      ],
      block("", "", ""),
    )
  );
  assert_eq!(entities.len(), 9);
  assert_eq!(
    entities[0],
    entity(
      ("cs1_railwyc_trk05", 1572872, 3665004499),
      position(-544.0997, 4910.33, 89.414536),
      (8, 299.0, 0.0),
      ("LODTYPES_DEPTH_HD", 0),
    )
  );
  assert_eq!(
    entities[1],
    entity(
      ("cs1_railwyc_trk07", 1572872, 541857008),
      position(-537.1905, 4670.2344, 88.41368),
      (7, 299.0, 0.0),
      ("LODTYPES_DEPTH_HD", 0),
    )
  );
}

#[test]
fn decodes_id2_17_without_entities() {
  assert_eq!(
    read("id2_17"),
    header(
      ("id2_17", "", 2, 16),
      std::array::from_fn(|_| position(0.0, 0.0, 0.0)),
      block("id2_17", "rtavares", "Friday, December 16, 2011 4:32 PM"),
    )
  );
}

/// This older schema has no `childLodDist`, so it decodes as its default.
#[test]
fn decodes_id2_17_strm_0_from_its_older_entity_schema() {
  let (model, entities) = split(read("id2_17_strm_0"));
  assert_eq!(
    model,
    header(
      ("id2_17_strm_0", "id2_17", 0, 1),
      [
        position(1108.5085, -970.8059, -65.188675),
        position(1308.5085, -770.8059, 134.81133),
        position(1208.0085, -871.3059, 34.811325),
        position(1209.0085, -870.3059, 35.811325),
      ],
      block("", "", ""),
    )
  );
  assert_eq!(
    entities,
    [entity(
      ("id2_17_void", 1572864, 0),
      position(1208.5085, -870.8059, 34.811325),
      (-1, 100.0, 0.0),
      ("LODTYPES_DEPTH_ORPHANHD", 0),
    )]
  );
}
