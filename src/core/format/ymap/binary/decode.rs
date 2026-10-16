//! Direct typed decoding of a `CMapData` root into a YMAP model.
//!
//! Works over any [`StructReader`], so RSC7 META and PSO share one mapping. Field requirements
//! mirror the `XmlYmap` serde models: fields those models default stay optional here, so old
//! schemas (e.g. PSO without `childLodDist`) still decode.

use std::io;

use super::read::invalid_data;
use crate::common::{position::Position, rotation::Rotation, triangle::Triangle};
use crate::core::format::{
  gamefile::struct_reader::{FromScalar, Scalar, StructReader},
  ymap::model::{
    BoundingBox, GrassInstance, GrassInstanceBatch, Vector3, Vector4, Ymap, YmapBlock,
    YmapBoxOccluder, YmapCarGenerator, YmapContainerLod, YmapDistantLodLight, YmapDistantLodLights,
    YmapEntity, YmapInstancedData, YmapLodLight, YmapOccludeModel, YmapTimeCycleModifier,
  },
};

/// Decodes a `CMapData` root structure into a YMAP model.
pub(super) fn ymap_from_root<S: StructReader>(root: &S) -> io::Result<Ymap> {
  Ok(Ymap {
    name: root.get("name")?,
    parent: root.get("parent")?,
    flags: root.get("flags")?,
    content_flags: root.get("contentFlags")?,
    streaming_extents_min: position(root, "streamingExtentsMin")?,
    streaming_extents_max: position(root, "streamingExtentsMax")?,
    entities_extents_min: position(root, "entitiesExtentsMin")?,
    entities_extents_max: position(root, "entitiesExtentsMax")?,
    entities: collect(root.children("entities")?, entity)?,
    box_occluders: collect(root.children("boxOccluders")?, box_occluder)?,
    occlude_models: collect(root.children("occludeModels")?, occlude_model)?,
    lod_lights: match root.child("LODLightsSOA")? {
      Some(soa) => lod_lights(&soa)?,
      None => Vec::new(),
    },
    distant_lod_lights: match root.child("DistantLODLightsSOA")? {
      Some(soa) => distant_lod_lights(&soa)?,
      None => YmapDistantLodLights::default(),
    },
    block: block(&root.child("block")?.ok_or_else(|| invalid_data("CMapData.block is missing"))?)?,
    container_lods: collect(root.children("containerLods")?, container_lod)?,
    physics_dictionaries: root.list("physicsDictionaries")?,
    instanced_data: match root.child("instancedData")? {
      Some(data) => instanced_data(&data)?,
      None => YmapInstancedData::default(),
    },
    time_cycle_modifiers: collect(root.children("timeCycleModifiers")?, time_cycle_modifier)?,
    car_generators: collect(root.children("carGenerators")?, car_generator)?,
  })
}

fn collect<S, T>(
  items: Vec<S>,
  convert: fn(&S) -> io::Result<T>,
) -> io::Result<Vec<T>> {
  items.iter().map(convert).collect()
}

fn entity<S: StructReader>(item: &S) -> io::Result<YmapEntity> {
  let [x, y, z, w] = vector::<4, _>(item, "rotation")?;
  Ok(YmapEntity {
    entity_type: item.type_name(),
    archetype_name: item.get("archetypeName")?,
    flags: item.get("flags")?,
    guid: guid(item.get("guid")?)?,
    position: position(item, "position")?,
    rotation: Rotation {
      x,
      y,
      z,
      w,
    },
    scale_x_y: item.get("scaleXY")?,
    scale_z: item.get("scaleZ")?,
    parent_index: item.get("parentIndex")?,
    lod_dist: item.get("lodDist")?,
    child_lod_dist: item.get_or_default("childLodDist")?,
    lod_level: item.get("lodLevel")?,
    num_children: item.get("numChildren")?,
    priority_level: item.get("priorityLevel")?,
    ambient_occlusion_multiplier: item.get("ambientOcclusionMultiplier")?,
    artificial_ambient_occlusion: item.get("artificialAmbientOcclusion")?,
    tint_value: item.get("tintValue")?,
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
    i_center_x: item.get("iCenterX")?,
    i_center_y: item.get("iCenterY")?,
    i_center_z: item.get("iCenterZ")?,
    i_cos_z: item.get("iCosZ")?,
    i_length: item.get("iLength")?,
    i_width: item.get("iWidth")?,
    i_height: item.get("iHeight")?,
    i_sin_z: item.get("iSinZ")?,
  })
}

/// Splits the packed occluder buffer into vertices (12-byte XYZ floats) and index triplets.
fn occlude_model<S: StructReader>(item: &S) -> io::Result<YmapOccludeModel> {
  let verts: Vec<u8> = item.list("verts")?;
  let split = item.get::<u32>("numVertsInBytes")? as usize;
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
    bmin: position(item, "bmin")?,
    bmax: position(item, "bmax")?,
    triangles,
    flags: item.get("flags")?,
  })
}

/// Expands the SOA arrays into one light per `direction`; other arrays may be longer.
fn lod_lights<S: StructReader>(soa: &S) -> io::Result<Vec<YmapLodLight>> {
  let direction = soa
    .children("direction")?
    .iter()
    .map(|item| {
      Ok(Position {
        x: item.get("x")?,
        y: item.get("y")?,
        z: item.get("z")?,
      })
    })
    .collect::<io::Result<Vec<_>>>()?;
  let count = direction.len();
  let values = |name: &str| -> io::Result<Vec<Scalar>> {
    let mut values = soa.values(name)?;
    if values.is_empty() && name == "hash" {
      values = soa.values("hash_0000004A")?;
    }
    if values.len() < count {
      return Err(invalid_data(&format!("LODLightsSOA.{name} is shorter than direction")));
    }
    Ok(values)
  };
  let falloff = values("falloff")?;
  let falloff_exponent = values("falloffExponent")?;
  let time_and_state_flags = values("timeAndStateFlags")?;
  let hash = values("hash")?;
  let cone_inner_angle = values("coneInnerAngle")?;
  let cone_outer_angle = values("coneOuterAngleOrCapExt")?;
  let corona_intensity = values("coronaIntensity")?;
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
    .children("position")?
    .iter()
    .map(|item| {
      let (x, y, z): (f32, f32, f32) = (item.get("x")?, item.get("y")?, item.get("z")?);
      Ok(format!("{x} {y} {z}"))
    })
    .collect::<io::Result<Vec<_>>>()?;
  Ok(YmapDistantLodLights {
    items: positions
      .into_iter()
      .zip(soa.values("RGBI")?)
      .map(|(position, rgbi)| YmapDistantLodLight {
        position,
        rgbi: rgbi.to_string(),
      })
      .collect(),
    num_street_lights: soa.get_or_default("numStreetLights")?,
    category: soa.get_or_default("category")?,
  })
}

fn block<S: StructReader>(item: &S) -> io::Result<YmapBlock> {
  Ok(YmapBlock {
    version: item.get("version")?,
    flags: item.get("flags")?,
    name: item.get("name")?,
    exported_by: item.get("exportedBy")?,
    owner: item.get("owner")?,
    time: item.get("time")?,
  })
}

fn container_lod<S: StructReader>(item: &S) -> io::Result<YmapContainerLod> {
  Ok(YmapContainerLod {
    name: item.get("name")?,
    parent_index: item.get("parentIndex")?,
  })
}

fn instanced_data<S: StructReader>(item: &S) -> io::Result<YmapInstancedData> {
  Ok(YmapInstancedData {
    imap_link: item.get_or_default("ImapLink")?,
    prop_instance_list: item.list("PropInstanceList")?,
    grass_instance_list: collect(item.children("GrassInstanceList")?, grass_batch)?,
  })
}

fn grass_batch<S: StructReader>(item: &S) -> io::Result<GrassInstanceBatch> {
  let (min, max) = match item.child("BatchAABB")? {
    Some(aabb) => {
      (vector_or_default::<4, _>(&aabb, "min")?, vector_or_default::<4, _>(&aabb, "max")?)
    }
    None => ([0.0; 4], [0.0; 4]),
  };
  let vector4 = |[x, y, z, w]: [f32; 4]| Vector4 {
    x,
    y,
    z,
    w,
  };
  let [x, y, z] = vector_or_default::<3, _>(item, "ScaleRange")?;
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
    archetype_name: item.get_or_default("archetypeName")?,
    lod_dist: item.get_or_default("lodDist")?,
    lod_fade_start_dist: item.get_or_default("LodFadeStartDist")?,
    lod_inst_fade_range: item.get_or_default("LodInstFadeRange")?,
    orient_to_terrain: item.get_or_default("OrientToTerrain")?,
    instances: collect(item.children("InstanceList")?, grass_instance)?,
  })
}

fn grass_instance<S: StructReader>(item: &S) -> io::Result<GrassInstance> {
  Ok(GrassInstance {
    position: item.list("Position")?,
    normal_x: item.get_or_default("NormalX")?,
    normal_y: item.get_or_default("NormalY")?,
    color: item.list("Color")?,
    scale: item.get_or_default("Scale")?,
    ao: item.get_or_default("Ao")?,
    pad: item.list("Pad")?,
  })
}

fn time_cycle_modifier<S: StructReader>(item: &S) -> io::Result<YmapTimeCycleModifier> {
  Ok(YmapTimeCycleModifier {
    name: item.get("name")?,
    min_extents: position(item, "minExtents")?,
    max_extents: position(item, "maxExtents")?,
    percentage: item.get("percentage")?,
    range: item.get("range")?,
    start_hour: item.get("startHour")?,
    end_hour: item.get("endHour")?,
  })
}

fn car_generator<S: StructReader>(item: &S) -> io::Result<YmapCarGenerator> {
  Ok(YmapCarGenerator {
    position: position(item, "position")?,
    orient_x: item.get("orientX")?,
    orient_y: item.get("orientY")?,
    perpendicular_length: item.get("perpendicularLength")?,
    car_model: item.get_or_default("carModel")?,
    flags: item.get("flags")?,
    body_color_remap_1: item.get("bodyColorRemap1")?,
    body_color_remap_2: item.get("bodyColorRemap2")?,
    body_color_remap_3: item.get("bodyColorRemap3")?,
    body_color_remap_4: item.get("bodyColorRemap4")?,
    pop_group: item.get_or_default("popGroup")?,
    livery: item.get("livery")?,
  })
}

fn position<S: StructReader>(
  item: &S,
  name: &str,
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
  name: &str,
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
  name: &str,
) -> io::Result<[f32; N]> {
  match item.vector(name)? {
    Some(_) => vector::<N, S>(item, name),
    None => Ok([0.0; N]),
  }
}
