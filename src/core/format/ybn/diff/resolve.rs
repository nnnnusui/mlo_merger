//! Polygons resolved by value from the tables of their geometry.

use super::polygon::{ResolvedPolygon, ResolvedShape};
use crate::core::format::ybn::model::{Geometry, Polygon};
use std::io;

fn invalid(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

/// `polygon` of `geometry` with its vertices, material and colours resolved by value, positions
/// offset by the geometry's centre.
pub(super) fn resolved(
  geometry: &Geometry,
  polygon: &Polygon,
) -> io::Result<ResolvedPolygon> {
  let mut colours = Vec::new();
  if !geometry.vertex_colours.is_empty() && geometry.vertex_colours.len() != geometry.vertices.len()
  {
    return Err(invalid("YBN vertex colour table does not match vertices"));
  }
  let mut vertex = |index: u16| -> io::Result<[f32; 3]> {
    let value = geometry
      .vertices
      .get(index as usize)
      .ok_or_else(|| invalid("YBN polygon vertex index out of range"))?;
    let position = std::array::from_fn(|axis| value[axis] + geometry.center[axis]);
    if position.iter().any(|value| !value.is_finite()) {
      return Err(invalid("Non-finite YBN vertex"));
    }
    if !geometry.vertex_colours.is_empty() {
      colours.push(geometry.vertex_colours[index as usize]);
    }
    Ok(position)
  };
  let shape = match polygon {
    Polygon::Triangle(triangle) => ResolvedShape::Triangle {
      vertices: [
        vertex(triangle.vertices[0])?,
        vertex(triangle.vertices[1])?,
        vertex(triangle.vertices[2])?,
      ],
      vertex_flags: triangle.vertex_flags,
    },
    Polygon::Box {
      vertices,
      ..
    } => ResolvedShape::Box {
      vertices: [
        vertex(vertices[0])?,
        vertex(vertices[1])?,
        vertex(vertices[2])?,
        vertex(vertices[3])?,
      ],
    },
    Polygon::Sphere {
      vertex: index,
      radius,
      ..
    } => ResolvedShape::Sphere {
      center: vertex(*index)?,
      radius: *radius,
    },
    Polygon::Capsule {
      vertex1,
      vertex2,
      radius,
      ..
    } => ResolvedShape::Capsule {
      endpoints: [vertex(*vertex1)?, vertex(*vertex2)?],
      radius: *radius,
    },
    Polygon::Cylinder {
      vertex1,
      vertex2,
      radius,
      ..
    } => ResolvedShape::Cylinder {
      endpoints: [vertex(*vertex1)?, vertex(*vertex2)?],
      radius: *radius,
    },
    Polygon::Unsupported {
      ..
    } => return Err(invalid("Opaque YBN polygons cannot be semantically diffed")),
  };
  if let ResolvedShape::Sphere {
    radius,
    ..
  }
  | ResolvedShape::Capsule {
    radius,
    ..
  }
  | ResolvedShape::Cylinder {
    radius,
    ..
  } = &shape
    && (!radius.is_finite() || *radius < 0.0)
  {
    return Err(invalid("Invalid YBN polygon radius"));
  }
  let mut material = geometry
    .materials
    .get(polygon.material() as usize)
    .copied()
    .ok_or_else(|| invalid("YBN material index out of range"))?;
  let material_colour = geometry.material_colours.get(material.colour_index as usize).copied();
  if material_colour.is_some() {
    material.colour_index = 0;
  }
  Ok(ResolvedPolygon {
    shape,
    material,
    material_colour,
    vertex_colours: colours,
  })
}
