//! Reads native Bounds, shared tables and polygon records.

use super::super::*;

/// Reads the codec-supported Bounds hierarchy from a native YBN resource.
pub fn read_ybn_root(bytes: &[u8]) -> io::Result<Bound> {
  let resource = Rsc7Resource::decode(bytes)?;
  read_bound(&resource, BASE + ROOT_OFFSET as u64, None)
}

pub(in crate::core::format::ybn) fn read_bound(
  resource: &Rsc7Resource,
  address: u64,
  parent_transform: Option<[f32; 16]>,
) -> io::Result<Bound> {
  if address == 0 {
    return Ok(empty_bound(parent_transform));
  }
  let common = resource.read_address(address, BOUNDS_SIZE.min(112))?.to_vec();
  let kind = bound_type(common[16])?;
  if kind == "None" {
    return Ok(empty_bound(parent_transform));
  }
  let total_size = match kind.as_str() {
    "Composite" => COMPOSITE_SIZE,
    "Geometry" => GEOMETRY_SIZE,
    "GeometryBVH" => GEOMETRY_BVH_SIZE,
    "Capsule" | "Disc" | "Cylinder" => 128,
    "Sphere" | "Box" | "Cloth" => BOUNDS_SIZE,
    _ => return Err(invalid(&format!("Native YBN does not support Bounds type {kind}"))),
  };
  let record = resource.read_address(address, total_size)?;
  let mut bound = Bound {
    kind: kind.clone(),
    common: record[..112].to_vec(),
    extension: record[112..].to_vec(),
    geometry: None,
    children: Vec::new(),
    transform: parent_transform,
    composite_flags: [0; 2],
    transforms: Vec::new(),
    flags: Vec::new(),
  };
  if kind == "Composite" {
    let children_pointer = read_u64(record, 112)?;
    let transform_pointer = read_u64(record, 120)?;
    let alternate_transform_pointer = read_u64(record, 128)?;
    let flags1_pointer = read_u64(record, 144)?;
    let flags2_pointer = read_u64(record, 152)?;
    let child_count = read_u16(record, 160)? as usize;
    let pointers = read_pointer_array(resource, children_pointer, child_count)?;
    let transform_pointer =
      if transform_pointer == 0 { alternate_transform_pointer } else { transform_pointer };
    let transform_bytes =
      read_optional_address_table(resource, transform_pointer, child_count, 64)?;
    let flags1 = read_optional_address_table(resource, flags1_pointer, child_count, 8)?;
    let _flags2 = read_optional_address_table(resource, flags2_pointer, child_count, 8)?;
    for (index, child_pointer) in pointers.iter().enumerate() {
      let transform = if transform_bytes.len() >= (index + 1) * 64 {
        read_matrix(&transform_bytes, index * 64)?
      } else {
        identity()
      };
      bound.transforms.push(transform);
      bound.flags.push([
        if flags1.len() >= (index + 1) * 8 { read_u32(&flags1, index * 8)? } else { 0 },
        if flags1.len() >= (index + 1) * 8 { read_u32(&flags1, index * 8 + 4)? } else { 0 },
      ]);
      let mut child = read_bound(resource, *child_pointer, Some(transform))?;
      child.composite_flags = bound.flags[index];
      bound.children.push(child);
    }
  } else if matches!(kind.as_str(), "Geometry" | "GeometryBVH") {
    bound.geometry = Some(read_geometry(resource, record, &kind)?);
  }
  Ok(bound)
}

pub(in crate::core::format::ybn) fn read_geometry(
  resource: &Rsc7Resource,
  record: &[u8],
  _kind: &str,
) -> io::Result<Geometry> {
  let mut geometry = Geometry {
    center: read_vec3(record, 160)?,
    unknown_9c: read_f32(record, 156)?,
    unknown_ac: read_f32(record, 172)?,
    ..Geometry::default()
  };
  let vertex_count = read_u32(record, 208)? as usize;
  let polygon_count = read_u32(record, 212)? as usize;
  let material_count = record[288] as usize;
  let material_colour_count = record[289] as usize;
  let vertices = read_address_table(resource, read_u64(record, 176)?, vertex_count, 6)?;
  let colors = read_optional_address_table(resource, read_u64(record, 184)?, vertex_count, 4)?;
  let materials = read_optional_address_table(
    resource,
    read_u64(record, 240)?,
    material_count.max(if material_count > 0 { 4 } else { 0 }),
    8,
  )?;
  let material_colours =
    read_optional_address_table(resource, read_u64(record, 248)?, material_colour_count, 4)?;
  let polygons = read_address_table(resource, read_u64(record, 136)?, polygon_count, 16)?;
  let polygon_material_indices =
    read_optional_address_table(resource, read_u64(record, 280)?, polygon_count, 1)?;
  let quantum = read_vec3(record, 144)?;
  geometry.vertex_quantum = Some(quantum);
  let mut world_vertices = Vec::with_capacity(vertex_count);
  for index in 0..vertex_count {
    world_vertices.push([
      read_i16(vertices, index * 6)? as f32 * quantum[0],
      read_i16(vertices, index * 6 + 2)? as f32 * quantum[1],
      read_i16(vertices, index * 6 + 4)? as f32 * quantum[2],
    ]);
  }
  geometry.vertices = world_vertices;
  geometry.vertex_colours = colors.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
  geometry.materials = materials
    .chunks_exact(8)
    .take(material_count)
    .map(|bytes| {
      let data1 = read_u32(bytes, 0).unwrap();
      let data2 = read_u32(bytes, 4).unwrap();
      Material {
        kind: data1 as u8,
        procedural_id: (data1 >> 8) as u8,
        room_id: ((data1 >> 16) & 0x1f) as u8,
        ped_density: ((data1 >> 21) & 7) as u8,
        flags: ((data1 >> 24) as u16) | ((data2 as u16 & 0xff) << 8),
        colour_index: (data2 >> 8) as u8,
        unknown: (data2 >> 16) as u16,
      }
    })
    .collect();
  geometry.material_colours =
    material_colours.chunks_exact(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
  for bytes in polygons.chunks_exact(16) {
    let material = polygon_material_indices.get(geometry.polygons.len()).copied().unwrap_or(0);
    match bytes[0] & 7 {
      0 => {
        let packed = [read_u16(bytes, 4)?, read_u16(bytes, 6)?, read_u16(bytes, 8)?];
        geometry.polygons.push(Polygon::Triangle(Triangle {
          material,
          area: read_f32(bytes, 0)?,
          vertices: [packed[0] & 0x7fff, packed[1] & 0x7fff, packed[2] & 0x7fff],
          vertex_flags: [packed[0] & 0x8000 != 0, packed[1] & 0x8000 != 0, packed[2] & 0x8000 != 0],
          edge_indices: [read_u16(bytes, 10)?, read_u16(bytes, 12)?, read_u16(bytes, 14)?],
        }));
      }
      3 => geometry.polygons.push(Polygon::Box {
        material,
        vertices: [
          read_u16(bytes, 4)?,
          read_u16(bytes, 6)?,
          read_u16(bytes, 8)?,
          read_u16(bytes, 10)?,
        ],
      }),
      1 => geometry.polygons.push(Polygon::Sphere {
        material,
        vertex: read_u16(bytes, 2)?,
        radius: read_f32(bytes, 4)?,
      }),
      2 => geometry.polygons.push(Polygon::Capsule {
        material,
        vertex1: read_u16(bytes, 2)?,
        radius: read_f32(bytes, 4)?,
        vertex2: read_u16(bytes, 8)?,
      }),
      4 => geometry.polygons.push(Polygon::Cylinder {
        material,
        vertex1: read_u16(bytes, 2)?,
        radius: read_f32(bytes, 4)?,
        vertex2: read_u16(bytes, 8)?,
      }),
      _ => geometry.polygons.push(Polygon::Unsupported {
        raw: bytes.try_into().map_err(|_| invalid("YBN polygon record has the wrong size"))?,
      }),
    }
  }
  Ok(geometry)
}
