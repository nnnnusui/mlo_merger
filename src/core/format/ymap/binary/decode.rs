//! Direct typed decoding of a `CMapData` root into a YMAP model.
//!
//! Works over any [`StructReader`], so RSC7 META and PSO share one mapping. Field requirements
//! mirror the `XmlYmap` serde models: fields those models default stay optional here, so old
//! schemas (e.g. PSO without `childLodDist`) still decode.

use std::io;

use super::read::invalid_data;
use crate::common::{position::Position, rotation::Rotation, triangle::Triangle};
use crate::core::format::{
  gamefile::{
    meta::jenk_hash,
    struct_reader::{FieldName, FromScalar, Scalar, StructReader, field},
  },
  ymap::model::{
    BoundingBox, GrassInstance, GrassInstanceBatch, Vector3, Vector4, Ymap, YmapBlock,
    YmapBoxOccluder, YmapCapsuleBound, YmapCarGenerator, YmapContainerLod, YmapDistantLodLight,
    YmapDistantLodLights, YmapDoor, YmapEntity, YmapExtension, YmapInstancedData, YmapLightAttr,
    YmapLightEffect, YmapLodLight, YmapOccludeModel, YmapSpawnPointOverride, YmapTimeCycleModifier,
    YmapVerletClothCustomBounds,
  },
};

/// Reads only the `entitiesExtentsMin` and `entitiesExtentsMax` of a `CMapData` root structure.
pub(super) fn entities_extents_from_root<S: StructReader>(
  root: &S
) -> io::Result<(Position, Position)> {
  Ok((position(root, field!("entitiesExtentsMin"))?, position(root, field!("entitiesExtentsMax"))?))
}

/// Decodes a `CMapData` root structure into a YMAP model.
pub(super) fn ymap_from_root<S: StructReader>(root: &S) -> io::Result<Ymap> {
  Ok(Ymap {
    name: root.get(field!("name"))?,
    parent: root.get(field!("parent"))?,
    flags: root.get(field!("flags"))?,
    content_flags: root.get(field!("contentFlags"))?,
    streaming_extents_min: position(root, field!("streamingExtentsMin"))?,
    streaming_extents_max: position(root, field!("streamingExtentsMax"))?,
    entities_extents_min: position(root, field!("entitiesExtentsMin"))?,
    entities_extents_max: position(root, field!("entitiesExtentsMax"))?,
    entities: collect(root.children(field!("entities"))?, entity)?,
    box_occluders: collect(root.children(field!("boxOccluders"))?, box_occluder)?,
    occlude_models: collect(root.children(field!("occludeModels"))?, occlude_model)?,
    lod_lights: match root.child(field!("LODLightsSOA"))? {
      Some(soa) => lod_lights(&soa)?,
      None => Vec::new(),
    },
    distant_lod_lights: match root.child(field!("DistantLODLightsSOA"))? {
      Some(soa) => distant_lod_lights(&soa)?,
      None => YmapDistantLodLights::default(),
    },
    block: block(
      &root.child(field!("block"))?.ok_or_else(|| invalid_data("CMapData.block is missing"))?,
    )?,
    container_lods: collect(root.children(field!("containerLods"))?, container_lod)?,
    physics_dictionaries: root.list(field!("physicsDictionaries"))?,
    instanced_data: match root.child(field!("instancedData"))? {
      Some(data) => instanced_data(&data)?,
      None => YmapInstancedData::default(),
    },
    time_cycle_modifiers: collect(
      root.children(field!("timeCycleModifiers"))?,
      time_cycle_modifier,
    )?,
    car_generators: collect(root.children(field!("carGenerators"))?, car_generator)?,
  })
}

fn collect<S, T>(
  items: Vec<S>,
  convert: fn(&S) -> io::Result<T>,
) -> io::Result<Vec<T>> {
  items.iter().map(convert).collect()
}

fn entity<S: StructReader>(item: &S) -> io::Result<YmapEntity> {
  let [x, y, z, w] = vector::<4, _>(item, field!("rotation"))?;
  Ok(YmapEntity {
    entity_type: item.type_name(),
    archetype_name: item.get(field!("archetypeName"))?,
    flags: item.get(field!("flags"))?,
    guid: guid(item.get(field!("guid"))?)?,
    position: position(item, field!("position"))?,
    rotation: Rotation {
      x,
      y,
      z,
      w,
    },
    scale_x_y: item.get(field!("scaleXY"))?,
    scale_z: item.get(field!("scaleZ"))?,
    parent_index: item.get(field!("parentIndex"))?,
    lod_dist: item.get(field!("lodDist"))?,
    child_lod_dist: item.get_or_default(field!("childLodDist"))?,
    lod_level: item.get(field!("lodLevel"))?,
    num_children: item.get(field!("numChildren"))?,
    priority_level: item.get(field!("priorityLevel"))?,
    ambient_occlusion_multiplier: item.get(field!("ambientOcclusionMultiplier"))?,
    artificial_ambient_occlusion: item.get(field!("artificialAmbientOcclusion"))?,
    tint_value: item.get(field!("tintValue"))?,
    extensions: collect(item.children(field!("extensions"))?, extension)?,
  })
}

/// Decodes one entity extension by its structure; other extension types are rejected rather
/// than dropped.
fn extension<S: StructReader>(item: &S) -> io::Result<YmapExtension> {
  let name = item.type_name();
  let hash = match name.strip_prefix("hash_") {
    Some(hex) => u32::from_str_radix(hex, 16).unwrap_or_default(),
    None => jenk_hash(&name),
  };
  Ok(match hash {
    LIGHT_EFFECT => YmapExtension::LightEffect(YmapLightEffect {
      name: item.get(field!("name"))?,
      offset_position: position(item, field!("offsetPosition"))?,
      instances: collect(item.children(field!("instances"))?, light_attr)?,
    }),
    SPAWN_POINT_OVERRIDE => YmapExtension::SpawnPointOverride(YmapSpawnPointOverride {
      name: item.get(field!("name"))?,
      offset_position: position(item, field!("offsetPosition"))?,
      scenario_type: item.get(field!("ScenarioType"))?,
      i_time_start_override: item.get(field!("iTimeStartOverride"))?,
      i_time_end_override: item.get(field!("iTimeEndOverride"))?,
      group: item.get(field!("Group"))?,
      model_set: item.get(field!("ModelSet"))?,
      availability_in_mp_sp: item.get(field!("AvailabilityInMpSp"))?,
      flags: item.get(field!("Flags"))?,
      radius: item.get(field!("Radius"))?,
      time_till_ped_leaves: item.get(field!("TimeTillPedLeaves"))?,
    }),
    DOOR => YmapExtension::Door(YmapDoor {
      name: item.get(field!("name"))?,
      offset_position: position(item, field!("offsetPosition"))?,
      enable_limit_angle: item.get(field!("enableLimitAngle"))?,
      starts_locked: item.get(field!("startsLocked"))?,
      can_break: item.get(field!("canBreak"))?,
      limit_angle: item.get(field!("limitAngle"))?,
      door_target_ratio: item.get(field!("doorTargetRatio"))?,
      audio_hash: item.get(field!("audioHash"))?,
    }),
    VERLET_CLOTH_CUSTOM_BOUNDS => {
      YmapExtension::VerletClothCustomBounds(YmapVerletClothCustomBounds {
        name: item.get(field!("name"))?,
        collision_data: collect(item.children(field!("CollisionData"))?, capsule_bound)?,
      })
    }
    _ => return Err(invalid_data(&format!("Entity extension {name} is not supported"))),
  })
}

const LIGHT_EFFECT: u32 = jenk_hash("CExtensionDefLightEffect");
const SPAWN_POINT_OVERRIDE: u32 = jenk_hash("CExtensionDefSpawnPointOverride");
const DOOR: u32 = jenk_hash("CExtensionDefDoor");
const VERLET_CLOTH_CUSTOM_BOUNDS: u32 = jenk_hash("rage__phVerletClothCustomBounds");

fn light_attr<S: StructReader>(item: &S) -> io::Result<YmapLightAttr> {
  Ok(YmapLightAttr {
    posn: array(item, field!("posn"))?,
    colour: array(item, field!("colour"))?,
    flashiness: item.get(field!("flashiness"))?,
    intensity: item.get(field!("intensity"))?,
    flags: item.get(field!("flags"))?,
    bone_tag: item.get(field!("boneTag"))?,
    light_type: item.get(field!("lightType"))?,
    group_id: item.get(field!("groupId"))?,
    time_flags: item.get(field!("timeFlags"))?,
    falloff: item.get(field!("falloff"))?,
    falloff_exponent: item.get(field!("falloffExponent"))?,
    culling_plane: array(item, field!("cullingPlane"))?,
    shadow_blur: item.get(field!("shadowBlur"))?,
    padding_1: item.get(field!("padding1"))?,
    padding_2: item.get(field!("padding2"))?,
    padding_3: item.get(field!("padding3"))?,
    vol_intensity: item.get(field!("volIntensity"))?,
    vol_size_scale: item.get(field!("volSizeScale"))?,
    vol_outer_colour: array(item, field!("volOuterColour"))?,
    light_hash: item.get(field!("lightHash"))?,
    vol_outer_intensity: item.get(field!("volOuterIntensity"))?,
    corona_size: item.get(field!("coronaSize"))?,
    vol_outer_exponent: item.get(field!("volOuterExponent"))?,
    light_fade_distance: item.get(field!("lightFadeDistance"))?,
    shadow_fade_distance: item.get(field!("shadowFadeDistance"))?,
    specular_fade_distance: item.get(field!("specularFadeDistance"))?,
    volumetric_fade_distance: item.get(field!("volumetricFadeDistance"))?,
    shadow_near_clip: item.get(field!("shadowNearClip"))?,
    corona_intensity: item.get(field!("coronaIntensity"))?,
    corona_z_bias: item.get(field!("coronaZBias"))?,
    direction: array(item, field!("direction"))?,
    tangent: array(item, field!("tangent"))?,
    cone_inner_angle: item.get(field!("coneInnerAngle"))?,
    cone_outer_angle: item.get(field!("coneOuterAngle"))?,
    extents: array(item, field!("extents"))?,
    projected_texture_key: item.get(field!("projectedTextureKey"))?,
  })
}

fn capsule_bound<S: StructReader>(item: &S) -> io::Result<YmapCapsuleBound> {
  Ok(YmapCapsuleBound {
    owner_name: item.get(field!("OwnerName"))?,
    rotation: vector::<4, _>(item, field!("Rotation"))?,
    position: position(item, field!("Position"))?,
    normal: position(item, field!("Normal"))?,
    capsule_radius: item.get(field!("CapsuleRadius"))?,
    capsule_len: item.get(field!("CapsuleLen"))?,
    capsule_half_height: item.get(field!("CapsuleHalfHeight"))?,
    capsule_half_width: item.get(field!("CapsuleHalfWidth"))?,
    flags: item.get(field!("Flags"))?,
  })
}

/// Reads a fixed-size inline array field of `N` items.
fn array<T: FromScalar, const N: usize, S: StructReader>(
  item: &S,
  name: FieldName,
) -> io::Result<[T; N]> {
  let values = item.list::<T>(name)?;
  let count = values.len();
  values.try_into().map_err(|_| {
    invalid_data(&format!("{}.{name} has {count} items, expected {N}", item.type_name()))
  })
}

/// GUIDs are stored as signed or unsigned 32-bit values; keep their bit pattern.
fn guid(value: Scalar) -> io::Result<u32> {
  match value {
    Scalar::Int(value) if value < 0 => i32::try_from(value)
      .map(|value| value as u32)
      .map_err(|_| invalid_data("entity guid is outside the 32-bit range")),
    other => u32::from_scalar(other),
  }
}

fn box_occluder<S: StructReader>(item: &S) -> io::Result<YmapBoxOccluder> {
  Ok(YmapBoxOccluder {
    i_center_x: item.get(field!("iCenterX"))?,
    i_center_y: item.get(field!("iCenterY"))?,
    i_center_z: item.get(field!("iCenterZ"))?,
    i_cos_z: item.get(field!("iCosZ"))?,
    i_length: item.get(field!("iLength"))?,
    i_width: item.get(field!("iWidth"))?,
    i_height: item.get(field!("iHeight"))?,
    i_sin_z: item.get(field!("iSinZ"))?,
  })
}

/// Splits the packed occluder buffer into vertices (12-byte XYZ floats) and index triplets.
fn occlude_model<S: StructReader>(item: &S) -> io::Result<YmapOccludeModel> {
  let verts: Vec<u8> = item.list(field!("verts"))?;
  let split = item.get::<u32>(field!("numVertsInBytes"))? as usize;
  let (vertex_bytes, indices) = verts
    .split_at_checked(split)
    .ok_or_else(|| invalid_data("occluder vertex size exceeds its buffer"))?;
  let vertices = vertex_bytes
    .as_chunks::<4>()
    .0
    .as_chunks::<3>()
    .0
    .iter()
    .map(|[x, y, z]| Position {
      x: f32::from_le_bytes(*x),
      y: f32::from_le_bytes(*y),
      z: f32::from_le_bytes(*z),
    })
    .collect::<Vec<_>>();
  let corner = |index: u8| {
    vertices
      .get(index as usize)
      .cloned()
      .ok_or_else(|| invalid_data("occluder index references a missing vertex"))
  };
  let triangles = indices
    .as_chunks::<3>()
    .0
    .iter()
    .map(|&[a, b, c]| {
      Ok(Triangle {
        corner_1: corner(a)?,
        corner_2: corner(b)?,
        corner_3: corner(c)?,
      })
    })
    .collect::<io::Result<Vec<_>>>()?;
  Ok(YmapOccludeModel {
    bmin: position(item, field!("bmin"))?,
    bmax: position(item, field!("bmax"))?,
    triangles,
    flags: item.get(field!("flags"))?,
  })
}

/// Expands the SOA arrays into one light per `direction`; other arrays may be longer.
fn lod_lights<S: StructReader>(soa: &S) -> io::Result<Vec<YmapLodLight>> {
  let direction = soa
    .children(field!("direction"))?
    .iter()
    .map(|item| {
      Ok(Position {
        x: item.get(field!("x"))?,
        y: item.get(field!("y"))?,
        z: item.get(field!("z"))?,
      })
    })
    .collect::<io::Result<Vec<_>>>()?;
  let count = direction.len();
  let values = |name: FieldName| -> io::Result<Vec<Scalar>> {
    let mut values = soa.values(name)?;
    if values.is_empty() && name.name == "hash" {
      values = soa.values(field!("hash_0000004A"))?;
    }
    if values.len() < count {
      return Err(invalid_data(&format!("LODLightsSOA.{name} is shorter than direction")));
    }
    Ok(values)
  };
  let falloff = values(field!("falloff"))?;
  let falloff_exponent = values(field!("falloffExponent"))?;
  let time_and_state_flags = values(field!("timeAndStateFlags"))?;
  let hash = values(field!("hash"))?;
  let cone_inner_angle = values(field!("coneInnerAngle"))?;
  let cone_outer_angle = values(field!("coneOuterAngleOrCapExt"))?;
  let corona_intensity = values(field!("coronaIntensity"))?;
  direction
    .into_iter()
    .enumerate()
    .map(|(index, direction)| {
      Ok(YmapLodLight {
        direction,
        falloff: f32::from_scalar(falloff[index].clone())?,
        falloff_exponent: f32::from_scalar(falloff_exponent[index].clone())?,
        time_and_state_flags: time_and_state_flags[index].to_string(),
        hash: hash[index].to_string(),
        cone_inner_angle: u32::from_scalar(cone_inner_angle[index].clone())?,
        cone_outer_angle_or_cap_ext: u32::from_scalar(cone_outer_angle[index].clone())?,
        corona_intensity: f32::from_scalar(corona_intensity[index].clone())?,
      })
    })
    .collect()
}

/// Pairs positions with RGBI values; unmatched trailing entries are dropped.
fn distant_lod_lights<S: StructReader>(soa: &S) -> io::Result<YmapDistantLodLights> {
  let positions = soa
    .children(field!("position"))?
    .iter()
    .map(|item| {
      let (x, y, z): (f32, f32, f32) =
        (item.get(field!("x"))?, item.get(field!("y"))?, item.get(field!("z"))?);
      Ok(format!("{x} {y} {z}"))
    })
    .collect::<io::Result<Vec<_>>>()?;
  Ok(YmapDistantLodLights {
    items: positions
      .into_iter()
      .zip(soa.values(field!("RGBI"))?)
      .map(|(position, rgbi)| YmapDistantLodLight {
        position,
        rgbi: rgbi.to_string(),
      })
      .collect(),
    num_street_lights: soa.get_or_default(field!("numStreetLights"))?,
    category: soa.get_or_default(field!("category"))?,
  })
}

fn block<S: StructReader>(item: &S) -> io::Result<YmapBlock> {
  Ok(YmapBlock {
    version: item.get(field!("version"))?,
    flags: item.get(field!("flags"))?,
    name: item.get(field!("name"))?,
    exported_by: item.get(field!("exportedBy"))?,
    owner: item.get(field!("owner"))?,
    time: item.get(field!("time"))?,
  })
}

fn container_lod<S: StructReader>(item: &S) -> io::Result<YmapContainerLod> {
  Ok(YmapContainerLod {
    name: item.get(field!("name"))?,
    parent_index: item.get(field!("parentIndex"))?,
  })
}

fn instanced_data<S: StructReader>(item: &S) -> io::Result<YmapInstancedData> {
  Ok(YmapInstancedData {
    imap_link: item.get_or_default(field!("ImapLink"))?,
    prop_instance_list: item.list(field!("PropInstanceList"))?,
    grass_instance_list: collect(item.children(field!("GrassInstanceList"))?, grass_batch)?,
  })
}

fn grass_batch<S: StructReader>(item: &S) -> io::Result<GrassInstanceBatch> {
  let (min, max) = match item.child(field!("BatchAABB"))? {
    Some(aabb) => (
      vector_or_default::<4, _>(&aabb, field!("min"))?,
      vector_or_default::<4, _>(&aabb, field!("max"))?,
    ),
    None => ([0.0; 4], [0.0; 4]),
  };
  let vector4 = |[x, y, z, w]: [f32; 4]| Vector4 {
    x,
    y,
    z,
    w,
  };
  let [x, y, z] = vector_or_default::<3, _>(item, field!("ScaleRange"))?;
  Ok(GrassInstanceBatch {
    batch_aabb: BoundingBox {
      min: vector4(min),
      max: vector4(max),
    },
    scale_range: Vector3 {
      x,
      y,
      z,
    },
    archetype_name: item.get_or_default(field!("archetypeName"))?,
    lod_dist: item.get_or_default(field!("lodDist"))?,
    lod_fade_start_dist: item.get_or_default(field!("LodFadeStartDist"))?,
    lod_inst_fade_range: item.get_or_default(field!("LodInstFadeRange"))?,
    orient_to_terrain: item.get_or_default(field!("OrientToTerrain"))?,
    instances: collect(item.children(field!("InstanceList"))?, grass_instance)?,
  })
}

fn grass_instance<S: StructReader>(item: &S) -> io::Result<GrassInstance> {
  Ok(GrassInstance {
    position: array(item, field!("Position"))?,
    normal_x: item.get_or_default(field!("NormalX"))?,
    normal_y: item.get_or_default(field!("NormalY"))?,
    color: array(item, field!("Color"))?,
    scale: item.get_or_default(field!("Scale"))?,
    ao: item.get_or_default(field!("Ao"))?,
    pad: array(item, field!("Pad"))?,
  })
}

fn time_cycle_modifier<S: StructReader>(item: &S) -> io::Result<YmapTimeCycleModifier> {
  Ok(YmapTimeCycleModifier {
    name: item.get(field!("name"))?,
    min_extents: position(item, field!("minExtents"))?,
    max_extents: position(item, field!("maxExtents"))?,
    percentage: item.get(field!("percentage"))?,
    range: item.get(field!("range"))?,
    start_hour: item.get(field!("startHour"))?,
    end_hour: item.get(field!("endHour"))?,
  })
}

fn car_generator<S: StructReader>(item: &S) -> io::Result<YmapCarGenerator> {
  Ok(YmapCarGenerator {
    position: position(item, field!("position"))?,
    orient_x: item.get(field!("orientX"))?,
    orient_y: item.get(field!("orientY"))?,
    perpendicular_length: item.get(field!("perpendicularLength"))?,
    car_model: item.get_or_default(field!("carModel"))?,
    flags: item.get(field!("flags"))?,
    body_color_remap_1: item.get(field!("bodyColorRemap1"))?,
    body_color_remap_2: item.get(field!("bodyColorRemap2"))?,
    body_color_remap_3: item.get(field!("bodyColorRemap3"))?,
    body_color_remap_4: item.get(field!("bodyColorRemap4"))?,
    pop_group: item.get_or_default(field!("popGroup"))?,
    livery: item.get(field!("livery"))?,
  })
}

fn position<S: StructReader>(
  item: &S,
  name: FieldName,
) -> io::Result<Position> {
  let [x, y, z] = vector::<3, _>(item, name)?;
  Ok(Position {
    x,
    y,
    z,
  })
}

fn vector<const N: usize, S: StructReader>(
  item: &S,
  name: FieldName,
) -> io::Result<[f32; N]> {
  let values = item
    .vector(name)?
    .ok_or_else(|| invalid_data(&format!("{}.{name} is missing", item.type_name())))?;
  values.try_into().map_err(|values: Vec<f32>| {
    invalid_data(&format!(
      "{}.{name} has {} components, expected {N}",
      item.type_name(),
      values.len()
    ))
  })
}

fn vector_or_default<const N: usize, S: StructReader>(
  item: &S,
  name: FieldName,
) -> io::Result<[f32; N]> {
  match item.vector(name)? {
    Some(_) => vector::<N, S>(item, name),
    None => Ok([0.0; N]),
  }
}
