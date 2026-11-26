//! The YBN merge in use, taking each source's diff from the vanilla file directly, and the
//! Bounds helpers the polygon diff builds on.

use super::super::*;

/// Merges the changes of every YBN of `mods` from `vanilla`, as [`super::polygons`] compares,
/// combines and applies them, filling geometry children within `limits`.
pub fn merge_ybn_deltas(
  vanilla: &[u8],
  mods: &[&[u8]],
  limits: super::polygons::GeometryLimits,
) -> io::Result<Vec<u8>> {
  crate::core::config::matching::with_config(|| {
    let tolerance = crate::core::config::matching::current().ybn_position;
    let baseline = read_ybn_root(vanilla)?;
    let sources = mods.iter().map(|bytes| read_ybn_root(bytes)).collect::<io::Result<Vec<_>>>()?;
    let diffs = super::polygons::diff_each(&baseline, &sources, tolerance)?;
    merge_ybn_diffs(vanilla, &diffs, limits)
  })?
}

/// Applies the changes `diffs` make to `vanilla`, combined in order as [`super::polygons::merge`]
/// combines them, filling geometry children within `limits`.
pub fn merge_ybn_diffs(
  vanilla: &[u8],
  diffs: &[super::polygons::YbnDiffFields],
  limits: super::polygons::GeometryLimits,
) -> io::Result<Vec<u8>> {
  merge_ybn_diffs_with(vanilla, diffs, limits, false).map(|(merged, _)| merged)
}

/// [`merge_ybn_diffs`], leaving out the polygons repeating an earlier one of the same flags group
/// when `leave_out_repeats`; returns the merged YBN and how many polygons it left out.
pub fn merge_ybn_diffs_with(
  vanilla: &[u8],
  diffs: &[super::polygons::YbnDiffFields],
  limits: super::polygons::GeometryLimits,
  leave_out_repeats: bool,
) -> io::Result<(Vec<u8>, usize)> {
  crate::core::config::matching::with_config(|| {
    let tolerance = crate::core::config::matching::current().ybn_position;
    let mut merged = read_ybn_root(vanilla)?;
    if merged.kind != "Composite" {
      return Err(invalid("YBN delta merge requires a Composite vanilla root"));
    }
    let combined = super::polygons::merge(&diffs.iter().collect::<Vec<_>>(), tolerance)?;
    let left_out =
      super::polygons::apply_with(&mut merged, &combined, tolerance, limits, leave_out_repeats)?;
    Ok((encode_ybn_bound(&merged)?, left_out))
  })?
}

pub(in crate::core::format::ybn) fn is_polygon_geometry(bound: &Bound) -> bool {
  bound.geometry.as_ref().is_some_and(|geometry| {
    !geometry.polygons.is_empty()
      && geometry.polygons.iter().all(|polygon| !matches!(polygon, Polygon::Unsupported { .. }))
  })
}

pub(in crate::core::format::ybn) fn transform_ybn_point(
  point: [f32; 3],
  transform: Option<[f32; 16]>,
) -> [f32; 3] {
  let Some(matrix) = transform else {
    return point;
  };
  [
    point[0] * matrix[0] + point[1] * matrix[4] + point[2] * matrix[8] + matrix[12],
    point[0] * matrix[1] + point[1] * matrix[5] + point[2] * matrix[9] + matrix[13],
    point[0] * matrix[2] + point[1] * matrix[6] + point[2] * matrix[10] + matrix[14],
  ]
}

pub(in crate::core::format::ybn) fn geometry_polygon_subset(
  source: &Bound,
  selected: &HashSet<usize>,
) -> io::Result<Bound> {
  let source_geometry =
    source.geometry.as_ref().ok_or_else(|| invalid("YBN geometry is missing"))?;
  let has_vertex_colours = source_geometry.vertex_colours.len() == source_geometry.vertices.len();
  let mut vertex_map = HashMap::<u16, u16>::new();
  let mut vertices = Vec::new();
  let mut vertex_colours = Vec::new();
  let mut polygons = Vec::new();
  let mut polygon_map = HashMap::<usize, u16>::new();
  for (old_index, polygon) in source_geometry.polygons.iter().enumerate() {
    if selected.contains(&old_index) {
      let new_index =
        u16::try_from(polygons.len()).map_err(|_| invalid("YBN geometry has too many polygons"))?;
      polygon_map.insert(old_index, new_index);
      polygons.push(*polygon);
    }
  }
  for (new_index, polygon) in polygons.iter_mut().enumerate() {
    let mut remap_vertex = |index: &mut u16| -> io::Result<()> {
      let old_index = *index;
      let new_index = if let Some(index) = vertex_map.get(&old_index) {
        *index
      } else {
        let new_index = u16::try_from(vertices.len())
          .map_err(|_| invalid("YBN geometry has too many vertices"))?;
        let vertex = *source_geometry
          .vertices
          .get(old_index as usize)
          .ok_or_else(|| invalid("polygon vertex index is out of range"))?;
        vertices.push(vertex);
        if has_vertex_colours {
          vertex_colours.push(source_geometry.vertex_colours[old_index as usize]);
        }
        vertex_map.insert(old_index, new_index);
        new_index
      };
      *index = new_index;
      Ok(())
    };
    match polygon {
      Polygon::Triangle(triangle) => {
        for vertex in &mut triangle.vertices {
          remap_vertex(vertex)?;
        }
        for edge in &mut triangle.edge_indices {
          if *edge != u16::MAX {
            *edge = polygon_map.get(&(*edge as usize)).copied().unwrap_or(u16::MAX);
          }
        }
      }
      Polygon::Box {
        vertices,
        ..
      } => {
        for vertex in vertices {
          remap_vertex(vertex)?;
        }
      }
      Polygon::Sphere {
        vertex,
        ..
      } => remap_vertex(vertex)?,
      Polygon::Capsule {
        vertex1,
        vertex2,
        ..
      }
      | Polygon::Cylinder {
        vertex1,
        vertex2,
        ..
      } => {
        remap_vertex(vertex1)?;
        remap_vertex(vertex2)?;
      }
      Polygon::Unsupported {
        ..
      } => {
        return Err(invalid("unsupported YBN polygon cannot be subset"));
      }
    }
    let _ = new_index;
  }

  let mut geometry = source_geometry.clone();
  geometry.vertices = vertices;
  if has_vertex_colours {
    geometry.vertex_colours = vertex_colours;
  }
  geometry.polygons = polygons;
  let mut result = source.clone();
  result.geometry = Some(geometry);
  Ok(result)
}

pub(in crate::core::format::ybn) fn bound_identity(bound: &Bound) -> io::Result<Vec<u8>> {
  let mut identity = Vec::new();
  append_bound_identity(bound, &mut identity)?;
  Ok(identity)
}

pub(in crate::core::format::ybn) fn append_bound_identity(
  bound: &Bound,
  identity: &mut Vec<u8>,
) -> io::Result<()> {
  identity.extend_from_slice(&(bound.kind.len() as u32).to_le_bytes());
  identity.extend_from_slice(bound.kind.as_bytes());
  identity.extend_from_slice(&bound.common[60..64]);
  identity.extend_from_slice(&bound.common[76..80]);
  identity.extend_from_slice(&bound.common[92..94]);
  identity.extend_from_slice(&bound.composite_flags[0].to_le_bytes());
  identity.extend_from_slice(&bound.composite_flags[1].to_le_bytes());
  if let Some(transform) = bound.transform {
    identity.push(1);
    for value in transform {
      append_quantized_float(identity, value)?;
    }
  } else {
    identity.push(0);
  }

  if let Some(geometry) = &bound.geometry {
    identity.push(1);
    for value in geometry
      .center
      .iter()
      .copied()
      .chain([geometry.unknown_9c, geometry.unknown_ac])
      .chain(geometry.vertices.iter().flatten().copied())
    {
      append_quantized_float(identity, value)?;
    }
    for material in &geometry.materials {
      identity.extend_from_slice(&[
        material.kind,
        material.procedural_id,
        material.room_id,
        material.ped_density,
      ]);
      identity.extend_from_slice(&material.flags.to_le_bytes());
      identity.push(material.colour_index);
      identity.extend_from_slice(&material.unknown.to_le_bytes());
    }
    for color in geometry.material_colours.iter().chain(&geometry.vertex_colours) {
      identity.extend_from_slice(color);
    }
    for polygon in &geometry.polygons {
      match polygon {
        Polygon::Triangle(triangle) => {
          identity.push(0);
          identity.push(triangle.material);
          for vertex in triangle.vertices {
            identity.extend_from_slice(&vertex.to_le_bytes());
          }
          for flag in triangle.vertex_flags {
            identity.push(u8::from(flag));
          }
        }
        Polygon::Box {
          material,
          vertices,
        } => {
          identity.extend_from_slice(&[1, *material]);
          for vertex in vertices {
            identity.extend_from_slice(&vertex.to_le_bytes());
          }
        }
        Polygon::Sphere {
          material,
          vertex,
          radius,
        } => {
          identity.extend_from_slice(&[2, *material]);
          identity.extend_from_slice(&vertex.to_le_bytes());
          append_quantized_float(identity, *radius)?;
        }
        Polygon::Capsule {
          material,
          vertex1,
          vertex2,
          radius,
        } => {
          identity.extend_from_slice(&[3, *material]);
          identity.extend_from_slice(&vertex1.to_le_bytes());
          identity.extend_from_slice(&vertex2.to_le_bytes());
          append_quantized_float(identity, *radius)?;
        }
        Polygon::Cylinder {
          material,
          vertex1,
          vertex2,
          radius,
        } => {
          identity.extend_from_slice(&[4, *material]);
          identity.extend_from_slice(&vertex1.to_le_bytes());
          identity.extend_from_slice(&vertex2.to_le_bytes());
          append_quantized_float(identity, *radius)?;
        }
        Polygon::Unsupported {
          raw,
        } => identity.extend_from_slice(raw),
      }
    }
  } else {
    identity.push(0);
    identity.extend_from_slice(&bound.common);
    if matches!(bound.kind.as_str(), "Capsule" | "Disc" | "Cylinder") {
      identity.extend_from_slice(&bound.extension);
    }
  }

  identity.extend_from_slice(&(bound.children.len() as u32).to_le_bytes());
  for (index, child) in bound.children.iter().enumerate() {
    for value in bound.transforms[index] {
      append_quantized_float(identity, value)?;
    }
    identity.extend_from_slice(&bound.flags[index][0].to_le_bytes());
    identity.extend_from_slice(&bound.flags[index][1].to_le_bytes());
    append_bound_identity(child, identity)?;
  }
  Ok(())
}

pub(in crate::core::format::ybn) fn append_quantized_float(
  identity: &mut Vec<u8>,
  value: f32,
) -> io::Result<()> {
  if !value.is_finite() {
    return Err(invalid("YBN identity contains a non-finite float"));
  }
  let quantized = (value as f64 * 1000.0).round() as i64;
  identity.extend_from_slice(&quantized.to_le_bytes());
  Ok(())
}
