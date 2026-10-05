use super::polygon::{PolygonLocation, ResolvedPolygon, ResolvedShape};
use crate::core::format::ybn::model::{Bound, Geometry, Polygon};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;

/// A primitive Bounds shape's spatial parameters, not GeometryBVH acceleration bounds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrimitiveBounds {
  /// Native minimum extents.
  pub minimum: [f32; 3],
  /// Native maximum extents.
  pub maximum: [f32; 3],
  /// Native box center.
  pub box_center: [f32; 3],
  /// Native sphere center.
  pub sphere_center: [f32; 3],
  /// Native sphere radius.
  pub sphere_radius: f32,
  /// Preserved primitive-specific extension.
  pub extension: Vec<u8>,
}

/// Input/collision metadata kept separate from polygon geometry and derived BVH data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundMetadata {
  /// Native Bounds kind.
  pub kind: String,
  /// Local parent transform.
  pub transform: Option<[f32; 16]>,
  /// Local collision filters.
  pub composite_flags: [u32; 2],
  /// Composite child transforms, preserving empty slots and hierarchy ownership.
  pub child_transforms: Vec<[f32; 16]>,
  /// Composite child filter flags.
  pub child_flags: Vec<[u32; 2]>,
  /// Collision margin.
  pub margin: f32,
  /// Native volume metadata.
  pub volume: f32,
  /// Native inertia metadata.
  pub inertia: [f32; 3],
  /// Packed material/room/filter metadata, excluding pointer and acceleration fields.
  pub attributes: Vec<u8>,
  /// Preserved native unknown type bits.
  pub unknown_type: u32,
  /// Geometry-specific unknown scalar metadata.
  pub geometry_unknowns: Option<[f32; 2]>,
  /// Spatial values for primitive Bounds, absent for geometry and composites.
  pub primitive: Option<PrimitiveBounds>,
}

pub(super) struct Record {
  pub location: PolygonLocation,
  pub polygon: ResolvedPolygon,
}

type BoundIndex = BTreeMap<Vec<usize>, BoundMetadata>;

fn invalid(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

fn float(
  bytes: &[u8],
  offset: usize,
) -> io::Result<f32> {
  let value = f32::from_le_bytes(
    bytes
      .get(offset..offset + 4)
      .ok_or_else(|| invalid("Truncated YBN metadata"))?
      .try_into()
      .unwrap(),
  );
  if !value.is_finite() {
    return Err(invalid("Non-finite YBN metadata"));
  }
  Ok(value)
}

fn vector(
  bytes: &[u8],
  offset: usize,
) -> io::Result<[f32; 3]> {
  Ok([float(bytes, offset)?, float(bytes, offset + 4)?, float(bytes, offset + 8)?])
}

fn metadata(bound: &Bound) -> io::Result<BoundMetadata> {
  if bound.common.len() < 112 {
    return Err(invalid("Truncated YBN Bounds metadata"));
  }
  if bound
    .transform
    .iter()
    .flatten()
    .chain(bound.transforms.iter().flatten())
    .any(|value| !value.is_finite())
  {
    return Err(invalid("Non-finite YBN transform"));
  }
  if bound.geometry.as_ref().is_some_and(|geometry| {
    [geometry.unknown_9c, geometry.unknown_ac].iter().any(|value| !value.is_finite())
  }) {
    return Err(invalid("Non-finite YBN geometry metadata"));
  }
  let primitive =
    if bound.geometry.is_none() && !matches!(bound.kind.as_str(), "Composite" | "None") {
      Some(PrimitiveBounds {
        minimum: vector(&bound.common, 48)?,
        maximum: vector(&bound.common, 32)?,
        box_center: vector(&bound.common, 64)?,
        sphere_center: vector(&bound.common, 80)?,
        sphere_radius: float(&bound.common, 20)?,
        extension: bound.extension.clone(),
      })
    } else {
      None
    };
  Ok(BoundMetadata {
    kind: bound.kind.clone(),
    transform: bound.transform,
    composite_flags: bound.composite_flags,
    child_transforms: bound.transforms.clone(),
    child_flags: bound.flags.clone(),
    margin: float(&bound.common, 44)?,
    volume: float(&bound.common, 108)?,
    inertia: vector(&bound.common, 96)?,
    attributes: bound.common[76..80].iter().chain(&bound.common[92..94]).copied().collect(),
    unknown_type: u32::from_le_bytes(bound.common[60..64].try_into().unwrap()),
    geometry_unknowns: bound
      .geometry
      .as_ref()
      .map(|geometry| [geometry.unknown_9c, geometry.unknown_ac]),
    primitive,
  })
}

fn resolved(
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
  let material_colour = if geometry.material_colours.is_empty() {
    None
  } else {
    let colour = *geometry
      .material_colours
      .get(material.colour_index as usize)
      .ok_or_else(|| invalid("YBN material colour index out of range"))?;
    material.colour_index = 0;
    Some(colour)
  };
  Ok(ResolvedPolygon {
    shape,
    material,
    material_colour,
    vertex_colours: colours,
  })
}

pub(super) fn collect(root: &Bound) -> io::Result<(BoundIndex, Vec<Record>)> {
  fn visit(
    bound: &Bound,
    path: &mut Vec<usize>,
    bounds: &mut BTreeMap<Vec<usize>, BoundMetadata>,
    records: &mut Vec<Record>,
  ) -> io::Result<()> {
    bounds.insert(path.clone(), metadata(bound)?);
    if let Some(geometry) = &bound.geometry {
      for (polygon_index, polygon) in geometry.polygons.iter().enumerate() {
        records.push(Record {
          location: PolygonLocation {
            bound_path: path.clone(),
            polygon_index,
          },
          polygon: resolved(geometry, polygon)?,
        });
      }
    }
    for (index, child) in bound.children.iter().enumerate() {
      path.push(index);
      visit(child, path, bounds, records)?;
      path.pop();
    }
    Ok(())
  }
  let mut bounds = BTreeMap::new();
  let mut records = Vec::new();
  visit(root, &mut Vec::new(), &mut bounds, &mut records)?;
  Ok((bounds, records))
}
