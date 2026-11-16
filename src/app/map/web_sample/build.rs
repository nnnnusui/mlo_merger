//! Builders for the web sample's models, filling the fields a sample does not care about.

use crate::common::{position::Position, rotation::Rotation, triangle::Triangle};
use crate::core::format::ymap::model::{
  ymap::Ymap,
  ymap_block::YmapBlock,
  ymap_box_occluder::YmapBoxOccluder,
  ymap_car_generator::YmapCarGenerator,
  ymap_distant_lod_lights::{YmapDistantLodLight, YmapDistantLodLights},
  ymap_entity::YmapEntity,
  ymap_extension::{
    YmapCapsuleBound, YmapDoor, YmapExtension, YmapLightAttr, YmapLightEffect,
    YmapSpawnPointOverride, YmapVerletClothCustomBounds,
  },
  ymap_instanced_data::{
    BoundingBox, GrassInstance, GrassInstanceBatch, Vector3, Vector4, YmapInstancedData,
  },
  ymap_lod_light::YmapLodLight,
  ymap_occlude_model::YmapOccludeModel,
  ymap_time_cycle_modifier::YmapTimeCycleModifier,
};

pub(super) fn position(
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

/// A rotation about the vertical axis by `degrees`.
pub(super) fn heading(degrees: f32) -> Rotation {
  let half = degrees.to_radians() / 2.0;
  Rotation {
    x: 0.0,
    y: 0.0,
    z: half.sin(),
    w: half.cos(),
  }
}

/// An empty map named `name`, its extents set by [`fit_extents`].
pub(super) fn ymap(
  name: &str,
  parent: &str,
  flags: u32,
  content_flags: u32,
) -> Ymap {
  Ymap {
    name: name.into(),
    parent: parent.into(),
    flags,
    content_flags,
    streaming_extents_min: position(0.0, 0.0, 0.0),
    streaming_extents_max: position(0.0, 0.0, 0.0),
    entities_extents_min: position(0.0, 0.0, 0.0),
    entities_extents_max: position(0.0, 0.0, 0.0),
    block: YmapBlock {
      version: 0,
      flags: 0,
      name: name.into(),
      exported_by: "mlo_merger web sample".into(),
      owner: String::new(),
      time: String::new(),
    },
    entities: Vec::new(),
    box_occluders: Vec::new(),
    occlude_models: Vec::new(),
    lod_lights: Vec::new(),
    distant_lod_lights: YmapDistantLodLights {
      items: Vec::new(),
      num_street_lights: 0,
      category: 0,
    },
    physics_dictionaries: Vec::new(),
    instanced_data: YmapInstancedData {
      imap_link: String::new(),
      prop_instance_list: Vec::new(),
      grass_instance_list: Vec::new(),
    },
    time_cycle_modifiers: Vec::new(),
    car_generators: Vec::new(),
    container_lods: Vec::new(),
  }
}

/// How far the sample takes each entity's archetype to reach from its position, for the entity
/// extents.
const ARCHETYPE_RADIUS: f32 = 10.0;

/// Sets the map's extents around its entities: their positions widened by
/// [`ARCHETYPE_RADIUS`] for the entity extents, and by their LOD distances for the streaming
/// extents.
pub(super) fn fit_extents(map: &mut Ymap) {
  let points = map
    .entities
    .iter()
    .map(|entity| (entity.position.clone(), entity.lod_dist))
    .chain(map.distant_lod_lights.items.iter().map(|light| {
      let mut parts = light.position.split(' ').map(|part| part.parse::<f32>().unwrap());
      (position(parts.next().unwrap(), parts.next().unwrap(), parts.next().unwrap()), 500.0)
    }))
    .collect::<Vec<_>>();
  if points.is_empty() {
    return;
  }
  let bound = |pick: fn(&Position) -> f32, widen: Option<f32>, minimum: bool| {
    points
      .iter()
      .map(|(point, distance)| {
        let value = pick(point);
        let offset = widen.map_or(ARCHETYPE_RADIUS, |widen| widen * distance);
        if minimum { value - offset } else { value + offset }
      })
      .fold(
        if minimum { f32::MAX } else { f32::MIN },
        |a, b| if minimum { a.min(b) } else { a.max(b) },
      )
  };
  let extents = |widen: Option<f32>| {
    (
      position(
        bound(|p| p.x, widen, true),
        bound(|p| p.y, widen, true),
        bound(|p| p.z, widen, true),
      ),
      position(
        bound(|p| p.x, widen, false),
        bound(|p| p.y, widen, false),
        bound(|p| p.z, widen, false),
      ),
    )
  };
  (map.entities_extents_min, map.entities_extents_max) = extents(None);
  (map.streaming_extents_min, map.streaming_extents_max) = extents(Some(1.0));
}

/// A high-detail entity without a LOD parent.
pub(super) fn entity(
  archetype: &str,
  guid: u32,
  at: Position,
  degrees: f32,
) -> YmapEntity {
  YmapEntity {
    entity_type: "CEntityDef".into(),
    archetype_name: archetype.into(),
    flags: 1_572_864,
    guid,
    position: at,
    rotation: heading(degrees),
    scale_x_y: 1.0,
    scale_z: 1.0,
    parent_index: -1,
    lod_dist: 120.0,
    child_lod_dist: 0.0,
    lod_level: "LODTYPES_DEPTH_HD".into(),
    num_children: 0,
    priority_level: "PRI_REQUIRED".into(),
    ambient_occlusion_multiplier: 255,
    artificial_ambient_occlusion: 255,
    tint_value: 0,
    extensions: Vec::new(),
  }
}

/// A LOD entity with `children` entities below it.
pub(super) fn lod_entity(
  archetype: &str,
  guid: u32,
  at: Position,
  level: &str,
  children: u32,
) -> YmapEntity {
  YmapEntity {
    flags: 1_572_872,
    lod_dist: 600.0,
    child_lod_dist: 120.0,
    lod_level: level.into(),
    num_children: children,
    ..entity(archetype, guid, at, 0.0)
  }
}

pub(super) fn door(
  name: &str,
  locked: bool,
) -> YmapExtension {
  YmapExtension::Door(YmapDoor {
    name: name.into(),
    offset_position: position(0.0, 0.0, 0.0),
    enable_limit_angle: true,
    starts_locked: locked,
    can_break: false,
    limit_angle: 90.0,
    door_target_ratio: 0.0,
    audio_hash: "smpl_door_audio".into(),
  })
}

pub(super) fn light_effect(
  name: &str,
  colour: [u8; 3],
) -> YmapExtension {
  YmapExtension::LightEffect(YmapLightEffect {
    name: name.into(),
    offset_position: position(0.0, 0.0, 0.0),
    instances: vec![YmapLightAttr {
      posn: [0.0, 0.0, 4.0],
      colour,
      flashiness: 0,
      intensity: 2.0,
      flags: 0,
      bone_tag: 0,
      light_type: 1,
      group_id: 0,
      time_flags: 16_646_400,
      falloff: 12.0,
      falloff_exponent: 8.0,
      culling_plane: [0.0, 0.0, -1.0, 0.0],
      shadow_blur: 0,
      padding_1: 0,
      padding_2: 0,
      padding_3: 0,
      vol_intensity: 0.0,
      vol_size_scale: 1.0,
      vol_outer_colour: [255, 255, 255],
      light_hash: 0,
      vol_outer_intensity: 1.0,
      corona_size: 1.0,
      vol_outer_exponent: 1.0,
      light_fade_distance: 0,
      shadow_fade_distance: 0,
      specular_fade_distance: 0,
      volumetric_fade_distance: 0,
      shadow_near_clip: 0.01,
      corona_intensity: 1.0,
      corona_z_bias: 0.1,
      direction: [0.0, 0.0, -1.0],
      tangent: [1.0, 0.0, 0.0],
      cone_inner_angle: 30.0,
      cone_outer_angle: 60.0,
      extents: [1.0, 1.0, 1.0],
      projected_texture_key: 0,
    }],
  })
}

/// A spawn point override available in both modes, without flags, as the decoder reads them.
pub(super) fn spawn_point(name: &str) -> YmapExtension {
  YmapExtension::SpawnPointOverride(YmapSpawnPointOverride {
    name: name.into(),
    offset_position: position(0.0, 0.6, 0.0),
    scenario_type: "smpl_sit_bench".into(),
    i_time_start_override: 6,
    i_time_end_override: 22,
    group: String::new(),
    model_set: String::new(),
    // Value 0 of an enum whose names are not known.
    availability_in_mp_sp: "hash_6E403D5A".into(),
    flags: String::new(),
    radius: 0.0,
    time_till_ped_leaves: 0.0,
  })
}

pub(super) fn cloth_bounds(name: &str) -> YmapExtension {
  YmapExtension::VerletClothCustomBounds(YmapVerletClothCustomBounds {
    name: name.into(),
    collision_data: vec![YmapCapsuleBound {
      owner_name: "smpl_flag_pole".into(),
      rotation: [0.0, 0.0, 0.0, 1.0],
      position: position(0.0, 0.0, 3.0),
      normal: position(0.0, 0.0, 1.0),
      capsule_radius: 0.1,
      capsule_len: 3.0,
      capsule_half_height: 0.0,
      capsule_half_width: 0.0,
      flags: String::new(),
    }],
  })
}

pub(super) fn car_generator(
  at: Position,
  model: &str,
) -> YmapCarGenerator {
  YmapCarGenerator {
    position: at,
    orient_x: 2.5,
    orient_y: 0.0,
    perpendicular_length: 1.25,
    car_model: model.into(),
    flags: 3_680,
    body_color_remap_1: -1,
    body_color_remap_2: -1,
    body_color_remap_3: -1,
    body_color_remap_4: -1,
    pop_group: String::new(),
    livery: -1,
  }
}

pub(super) fn time_cycle_modifier(
  name: &str,
  min: Position,
  max: Position,
  percentage: f32,
) -> YmapTimeCycleModifier {
  YmapTimeCycleModifier {
    name: name.into(),
    min_extents: min,
    max_extents: max,
    percentage,
    range: 10.0,
    start_hour: 0,
    end_hour: 23,
  }
}

/// A box occluder centred at `(x, y, z)` in metres, `length` by `width` by `height`.
pub(super) fn box_occluder(
  x: f32,
  y: f32,
  z: f32,
  size: [f32; 3],
) -> YmapBoxOccluder {
  YmapBoxOccluder {
    i_center_x: (x * 4.0) as i32,
    i_center_y: (y * 4.0) as i32,
    i_center_z: (z * 4.0) as i32,
    i_cos_z: 32_767,
    i_length: (size[0] * 4.0) as u32,
    i_width: (size[1] * 4.0) as u32,
    i_height: (size[2] * 4.0) as u32,
    i_sin_z: 0,
  }
}

/// An occlude model of two quads, a wall facing +Y and its roof, at `(x, y, z)`.
pub(super) fn occlude_model(
  x: f32,
  y: f32,
  z: f32,
) -> YmapOccludeModel {
  let corner = |dx: f32, dy: f32, dz: f32| position(x + dx, y + dy, z + dz);
  let quad = |a: Position, b: Position, c: Position, d: Position| {
    [
      Triangle {
        corner_1: a.clone(),
        corner_2: b,
        corner_3: c.clone(),
      },
      Triangle {
        corner_1: a,
        corner_2: c,
        corner_3: d,
      },
    ]
  };
  let mut triangles = Vec::new();
  triangles.extend(quad(
    corner(0.0, 0.0, 0.0),
    corner(8.0, 0.0, 0.0),
    corner(8.0, 0.0, 6.0),
    corner(0.0, 0.0, 6.0),
  ));
  triangles.extend(quad(
    corner(0.0, 0.0, 6.0),
    corner(8.0, 0.0, 6.0),
    corner(8.0, 6.0, 6.0),
    corner(0.0, 6.0, 6.0),
  ));
  YmapOccludeModel {
    bmin: corner(0.0, 0.0, 0.0),
    bmax: corner(8.0, 6.0, 6.0),
    triangles,
    flags: 0,
  }
}

/// A grass batch of `instances` tufts in a row along X through the middle of its box.
pub(super) fn grass(
  archetype: &str,
  min: Position,
  max: Position,
  instances: usize,
) -> GrassInstanceBatch {
  let vector = |p: &Position| Vector4 {
    x: p.x,
    y: p.y,
    z: p.z,
    w: 0.0,
  };
  GrassInstanceBatch {
    batch_aabb: BoundingBox {
      min: vector(&min),
      max: vector(&max),
    },
    scale_range: Vector3 {
      x: 0.8,
      y: 1.2,
      z: 0.5,
    },
    archetype_name: archetype.into(),
    lod_dist: 80.0,
    lod_fade_start_dist: 0.0,
    lod_inst_fade_range: 0.75,
    orient_to_terrain: 1,
    instances: (0..instances)
      .map(|index| {
        let along = (index as f32 + 0.5) / instances as f32;
        GrassInstance {
          // Packed into the batch's box, 0 to 65535 along each axis.
          position: [(along * 65535.0).round(), 32768.0, 0.0],
          normal_x: 128,
          normal_y: 128,
          color: [90, 140, 60],
          scale: 200,
          ao: 255,
          pad: [0; 3],
        }
      })
      .collect(),
  }
}

/// A LOD light pointing down; its distant partner is [`distant_light`] at the same index.
pub(super) fn lod_light(hash: u32) -> YmapLodLight {
  YmapLodLight {
    direction: position(0.0, 0.0, -1.0),
    falloff: 9.0,
    falloff_exponent: 32.0,
    time_and_state_flags: "16646400".into(),
    hash: hash.to_string(),
    cone_inner_angle: 12,
    cone_outer_angle_or_cap_ext: 64,
    corona_intensity: 4.0,
  }
}

pub(super) fn distant_light(
  at: Position,
  rgbi: u32,
) -> YmapDistantLodLight {
  YmapDistantLodLight {
    position: format!("{} {} {}", at.x, at.y, at.z),
    rgbi: rgbi.to_string(),
  }
}
