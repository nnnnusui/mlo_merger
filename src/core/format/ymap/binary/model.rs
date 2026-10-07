//! Converts ordered YMAP model collections to native META field values.

use super::super::model::{Ymap, YmapEntity};
use super::values::invalid;
use serde_json::Value;
use std::io;

/// Projects model collections into native light SOAs, grass records and occluder buffers.
pub(super) fn prepare(
  model: &Ymap,
  entities: &[YmapEntity],
) -> io::Result<Value> {
  let mut value = serde_json::to_value(model).map_err(invalid)?;
  let object = value.as_object_mut().ok_or_else(|| invalid("YMAP model is not an object"))?;
  object.remove("entity_map");
  object.insert("entities".into(), serde_json::to_value(entities).map_err(invalid)?);
  object.remove("lod_lights");
  object.remove("distant_lod_lights");
  let mut lights = serde_json::Map::new();
  for field in [
    "direction",
    "falloff",
    "falloff_exponent",
    "time_and_state_flags",
    "hash",
    "cone_inner_angle",
    "cone_outer_angle_or_cap_ext",
    "corona_intensity",
  ] {
    let values = model
      .lod_lights
      .iter()
      .map(|light| {
        let light = serde_json::to_value(light).map_err(invalid)?;
        Ok(light[field].clone())
      })
      .collect::<io::Result<Vec<_>>>()?;
    lights.insert(field.into(), Value::Array(values));
  }
  object.insert("LODLightsSOA".into(), Value::Object(lights));
  let positions = model
    .distant_lod_lights
    .items
    .iter()
    .map(|item| {
      let coordinates = item
        .position
        .split_whitespace()
        .map(|number| number.parse::<f32>().map_err(invalid))
        .collect::<io::Result<Vec<_>>>()?;
      if coordinates.len() != 3 {
        return Err(invalid("Distant light position requires three coordinates"));
      }
      Ok(serde_json::json!({"x": coordinates[0], "y": coordinates[1], "z": coordinates[2]}))
    })
    .collect::<io::Result<Vec<_>>>()?;
  object.insert("DistantLODLightsSOA".into(), serde_json::json!({
    "position": positions,
    "RGBI": model.distant_lod_lights.items.iter().map(|item| item.rgbi.clone()).collect::<Vec<_>>(),
    "num_street_lights": model.distant_lod_lights.num_street_lights,
    "category": model.distant_lod_lights.category,
  }));
  let occluders = model
    .occlude_models
    .iter()
    .map(|model| {
      let mut vertices = Vec::new();
      let mut indices = Vec::new();
      for triangle in &model.triangles {
        for vertex in [&triangle.corner_1, &triangle.corner_2, &triangle.corner_3] {
          let index = if let Some(index) = vertices.iter().position(|known| known == vertex) {
            index
          } else {
            vertices.push(vertex.clone());
            vertices.len() - 1
          };
          indices
            .push(u8::try_from(index).map_err(|_| invalid("Occluder has more than 256 vertices"))?);
        }
      }
      let mut bytes = vertices
        .iter()
        .flat_map(|vertex| [vertex.x, vertex.y, vertex.z])
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
      let vertex_bytes = bytes.len();
      bytes.extend(&indices);
      Ok(serde_json::json!({
        "bmin": model.bmin, "bmax": model.bmax, "verts": bytes,
        "data_size": bytes.len(), "num_verts_in_bytes": vertex_bytes,
        "num_tris": model.triangles.len() + 32768, "flags": model.flags,
      }))
    })
    .collect::<io::Result<Vec<_>>>()?;
  object.insert("occlude_models".into(), Value::Array(occluders));
  let grass = value["instanced_data"]["grass_instance_list"]
    .as_array_mut()
    .ok_or_else(|| invalid("Grass instance list is not an array"))?;
  for batch in grass {
    let batch = batch.as_object_mut().ok_or_else(|| invalid("Grass batch is not an object"))?;
    if let Some(instances) = batch.remove("instances") {
      batch.insert("InstanceList".into(), instances);
    }
  }
  Ok(value)
}
