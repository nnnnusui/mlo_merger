//! Direct typed encoding of a YMAP model into a `CMapData` root, the counterpart of `decode`.
//!
//! Each model type names the META fields it writes, as `decode` names those it reads;
//! [`StructWriter`] asks for them in schema order, and fields without a value stay zero.

use std::io;

use crate::common::position::Position;
use crate::core::format::{
  gamefile::{
    meta::jenk_hash,
    struct_writer::{Field, WriteFields},
  },
  ymap::model::{
    BoundingBox, GrassInstance, GrassInstanceBatch, Ymap, YmapBlock, YmapBoxOccluder,
    YmapCarGenerator, YmapContainerLod, YmapDistantLodLights, YmapEntity, YmapInstancedData,
    YmapLodLight, YmapOccludeModel, YmapTimeCycleModifier,
  },
};

/// Writes `$field` with the arm of its name, matched by Jenkins hash, or leaves it absent.
macro_rules! fields {
  ($field:ident { $($name:literal $(| $hash:literal)? => $write:expr),* $(,)? }) => {{
    let name = $field.name();
    $(if name == const { jenk_hash($name) } $(|| name == $hash)? {
      return $write;
    })*
    Ok(())
  }};
}

/// A `CMapData` root: the model with its entities in the order to write.
pub(super) struct Root<'a> {
  pub(super) model: &'a Ymap,
  pub(super) entities: &'a [YmapEntity],
}

impl WriteFields for Root<'_> {
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()> {
    let model = self.model;
    fields!(field {
      "name" => field.value(&model.name),
      "parent" => field.value(&model.parent),
      "flags" => field.value(model.flags),
      "contentFlags" => field.value(model.content_flags),
      "streamingExtentsMin" => field.vector(&xyz(&model.streaming_extents_min)),
      "streamingExtentsMax" => field.vector(&xyz(&model.streaming_extents_max)),
      "entitiesExtentsMin" => field.vector(&xyz(&model.entities_extents_min)),
      "entitiesExtentsMax" => field.vector(&xyz(&model.entities_extents_max)),
      "entities" => field.structures(self.entities.iter()),
      "containerLods" => field.structures(model.container_lods.iter()),
      "boxOccluders" => field.structures(model.box_occluders.iter()),
      "occludeModels" => {
        let models = model.occlude_models.iter().map(Occluder::new).collect::<io::Result<Vec<_>>>()?;
        field.structures(models.iter())
      },
      "physicsDictionaries" => field.values(model.physics_dictionaries.iter()),
      "instancedData" => field.structure(&model.instanced_data),
      "timeCycleModifiers" => field.structures(model.time_cycle_modifiers.iter()),
      "carGenerators" => field.structures(model.car_generators.iter()),
      "LODLightsSOA" => field.structure(&LodLights(&model.lod_lights)),
      "DistantLODLightsSOA" => field.structure(&DistantLodLights::new(&model.distant_lod_lights)?),
      "block" => field.structure(&model.block),
    })
  }
}

impl WriteFields for YmapEntity {
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()> {
    let rotation = &self.rotation;
    fields!(field {
      "archetypeName" => field.value(&self.archetype_name),
      "flags" => field.value(self.flags),
      "guid" => field.value(self.guid),
      "position" => field.vector(&xyz(&self.position)),
      "rotation" => field.vector(&[rotation.x, rotation.y, rotation.z, rotation.w]),
      "scaleXY" => field.value(self.scale_x_y),
      "scaleZ" => field.value(self.scale_z),
      "parentIndex" => field.value(self.parent_index),
      "lodDist" => field.value(self.lod_dist),
      "childLodDist" => field.value(self.child_lod_dist),
      "lodLevel" => field.value(&self.lod_level),
      "numChildren" => field.value(self.num_children),
      "priorityLevel" => field.value(&self.priority_level),
      "ambientOcclusionMultiplier" => field.value(self.ambient_occlusion_multiplier),
      "artificialAmbientOcclusion" => field.value(self.artificial_ambient_occlusion),
      "tintValue" => field.value(self.tint_value),
    })
  }

  fn type_name(&self) -> Option<&str> {
    Some(&self.entity_type)
  }
}

impl WriteFields for YmapBoxOccluder {
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()> {
    fields!(field {
      "iCenterX" => field.value(self.i_center_x),
      "iCenterY" => field.value(self.i_center_y),
      "iCenterZ" => field.value(self.i_center_z),
      "iCosZ" => field.value(self.i_cos_z),
      "iLength" => field.value(self.i_length),
      "iWidth" => field.value(self.i_width),
      "iHeight" => field.value(self.i_height),
      "iSinZ" => field.value(self.i_sin_z),
    })
  }
}

/// An occlude model with its triangles packed as `decode` unpacks them: distinct vertices as
/// XYZ floats, then one byte per corner indexing them.
struct Occluder<'a> {
  model: &'a YmapOccludeModel,
  verts: Vec<u8>,
  vertex_bytes: usize,
}

impl<'a> Occluder<'a> {
  fn new(model: &'a YmapOccludeModel) -> io::Result<Self> {
    let mut vertices = Vec::<&Position>::new();
    let mut indices = Vec::with_capacity(model.triangles.len() * 3);
    for triangle in &model.triangles {
      for vertex in [&triangle.corner_1, &triangle.corner_2, &triangle.corner_3] {
        let index = match vertices.iter().position(|known| *known == vertex) {
          Some(index) => index,
          None => {
            vertices.push(vertex);
            vertices.len() - 1
          }
        };
        indices
          .push(u8::try_from(index).map_err(|_| invalid("Occluder has more than 256 vertices"))?);
      }
    }
    let mut verts = vertices
      .iter()
      .flat_map(|vertex| [vertex.x, vertex.y, vertex.z])
      .flat_map(f32::to_le_bytes)
      .collect::<Vec<_>>();
    let vertex_bytes = verts.len();
    verts.extend(indices);
    Ok(Self {
      model,
      verts,
      vertex_bytes,
    })
  }
}

impl WriteFields for Occluder<'_> {
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()> {
    let model = self.model;
    fields!(field {
      "bmin" => field.vector(&xyz(&model.bmin)),
      "bmax" => field.vector(&xyz(&model.bmax)),
      "verts" => field.bytes(self.verts.clone()),
      "dataSize" => field.value(u32::try_from(self.verts.len()).map_err(invalid)?),
      "numVertsInBytes" => field.value(u32::try_from(self.vertex_bytes).map_err(invalid)?),
      "numTris" => field.value(u32::try_from(model.triangles.len() + 32768).map_err(invalid)?),
      "flags" => field.value(model.flags),
    })
  }
}

/// The lights as `LODLightsSOA`, one array per light field.
struct LodLights<'a>(&'a [YmapLodLight]);

impl WriteFields for LodLights<'_> {
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()> {
    let lights = self.0;
    fields!(field {
      "direction" => field.structures(lights.iter().map(|light| Xyz(xyz(&light.direction)))),
      "falloff" => field.values(lights.iter().map(|light| light.falloff)),
      "falloffExponent" => field.values(lights.iter().map(|light| light.falloff_exponent)),
      "timeAndStateFlags" => field.values(lights.iter().map(|light| &light.time_and_state_flags)),
      // Some schemas leave the name of `hash` unresolved.
      "hash" | 0x4a => field.values(lights.iter().map(|light| &light.hash)),
      "coneInnerAngle" => field.values(lights.iter().map(|light| light.cone_inner_angle)),
      "coneOuterAngleOrCapExt" => {
        field.values(lights.iter().map(|light| light.cone_outer_angle_or_cap_ext))
      },
      "coronaIntensity" => field.values(lights.iter().map(|light| light.corona_intensity)),
    })
  }
}

/// The distant lights as `DistantLODLightsSOA`, their `"x y z"` positions parsed.
struct DistantLodLights<'a> {
  lights: &'a YmapDistantLodLights,
  positions: Vec<[f32; 3]>,
}

impl<'a> DistantLodLights<'a> {
  fn new(lights: &'a YmapDistantLodLights) -> io::Result<Self> {
    let positions = lights
      .items
      .iter()
      .map(|item| {
        let coordinates = item
          .position
          .split_whitespace()
          .map(|number| number.parse::<f32>().map_err(invalid))
          .collect::<io::Result<Vec<_>>>()?;
        <[f32; 3]>::try_from(coordinates)
          .map_err(|_| invalid("Distant light position requires three coordinates"))
      })
      .collect::<io::Result<_>>()?;
    Ok(Self {
      lights,
      positions,
    })
  }
}

impl WriteFields for DistantLodLights<'_> {
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()> {
    let lights = self.lights;
    fields!(field {
      "position" => field.structures(self.positions.iter().map(|position| Xyz(*position))),
      "RGBI" => field.values(lights.items.iter().map(|item| &item.rgbi)),
      "numStreetLights" => field.value(lights.num_street_lights),
      "category" => field.value(lights.category),
    })
  }
}

/// A position written as an `x`, `y`, `z` structure.
struct Xyz([f32; 3]);

impl WriteFields for Xyz {
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()> {
    let [x, y, z] = self.0;
    fields!(field {
      "x" => field.value(x),
      "y" => field.value(y),
      "z" => field.value(z),
    })
  }
}

impl WriteFields for YmapBlock {
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()> {
    fields!(field {
      "version" => field.value(self.version),
      "flags" => field.value(self.flags),
      "name" => field.value(&self.name),
      "exportedBy" => field.value(&self.exported_by),
      "owner" => field.value(&self.owner),
      "time" => field.value(&self.time),
    })
  }
}

impl WriteFields for YmapContainerLod {
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()> {
    fields!(field {
      "name" => field.value(&self.name),
      "parentIndex" => field.value(self.parent_index),
    })
  }
}

impl WriteFields for YmapInstancedData {
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()> {
    fields!(field {
      "ImapLink" => field.value(&self.imap_link),
      "PropInstanceList" => field.values(self.prop_instance_list.iter()),
      "GrassInstanceList" => field.structures(self.grass_instance_list.iter()),
    })
  }
}

impl WriteFields for GrassInstanceBatch {
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()> {
    let range = &self.scale_range;
    fields!(field {
      "BatchAABB" => field.structure(&self.batch_aabb),
      "ScaleRange" => field.vector(&[range.x, range.y, range.z]),
      "archetypeName" => field.value(&self.archetype_name),
      "lodDist" => field.value(self.lod_dist),
      "LodFadeStartDist" => field.value(self.lod_fade_start_dist),
      "LodInstFadeRange" => field.value(self.lod_inst_fade_range),
      "OrientToTerrain" => field.value(self.orient_to_terrain),
      "InstanceList" => field.structures(self.instances.iter()),
    })
  }
}

impl WriteFields for BoundingBox {
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()> {
    let (min, max) = (&self.min, &self.max);
    fields!(field {
      "min" => field.vector(&[min.x, min.y, min.z, min.w]),
      "max" => field.vector(&[max.x, max.y, max.z, max.w]),
    })
  }
}

impl WriteFields for GrassInstance {
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()> {
    fields!(field {
      "Position" => field.values(self.position.iter().copied()),
      "NormalX" => field.value(self.normal_x),
      "NormalY" => field.value(self.normal_y),
      "Color" => field.values(self.color.iter().copied()),
      "Scale" => field.value(self.scale),
      "Ao" => field.value(self.ao),
      "Pad" => field.values(self.pad.iter().copied()),
    })
  }
}

impl WriteFields for YmapTimeCycleModifier {
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()> {
    fields!(field {
      "name" => field.value(&self.name),
      "minExtents" => field.vector(&xyz(&self.min_extents)),
      "maxExtents" => field.vector(&xyz(&self.max_extents)),
      "percentage" => field.value(self.percentage),
      "range" => field.value(self.range),
      "startHour" => field.value(self.start_hour),
      "endHour" => field.value(self.end_hour),
    })
  }
}

impl WriteFields for YmapCarGenerator {
  fn write_field(
    &self,
    field: &mut Field<'_, '_, '_>,
  ) -> io::Result<()> {
    fields!(field {
      "position" => field.vector(&xyz(&self.position)),
      "orientX" => field.value(self.orient_x),
      "orientY" => field.value(self.orient_y),
      "perpendicularLength" => field.value(self.perpendicular_length),
      "carModel" => field.value(&self.car_model),
      "flags" => field.value(self.flags),
      "bodyColorRemap1" => field.value(self.body_color_remap_1),
      "bodyColorRemap2" => field.value(self.body_color_remap_2),
      "bodyColorRemap3" => field.value(self.body_color_remap_3),
      "bodyColorRemap4" => field.value(self.body_color_remap_4),
      "popGroup" => field.value(&self.pop_group),
      "livery" => field.value(self.livery),
    })
  }
}

fn xyz(position: &Position) -> [f32; 3] {
  [position.x, position.y, position.z]
}

fn invalid(error: impl std::fmt::Display) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}
