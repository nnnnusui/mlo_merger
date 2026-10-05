//! Writes native Bounds and stable quantized GeometryBVH resources.

use super::*;

pub(in crate::core::format::ybn) fn encode_ybn_bound(bound: &Bound) -> io::Result<Vec<u8>> {
  let mut data = vec![0; ROOT_OFFSET + BOUNDS_SIZE];
  encode_bound(bound, &mut data, ROOT_OFFSET)?;
  let content_end = data.len();
  let mut slots = 1;
  let (page_info, count) = loop {
    data.truncate(content_end);
    let offset = reserve(&mut data, 16 + slots * 8)?;
    let probe = Rsc7Resource::from_pages(43, &data, &[])?;
    let actual = page_count(probe.system_flags).max(1);
    if actual <= slots {
      break (offset, actual);
    }
    slots = actual;
  };
  put_u32(&mut data, 0, 0x405b_c808)?;
  put_u32(&mut data, 4, 1)?;
  put_u64(&mut data, 8, address(page_info)?)?;
  data[page_info + 8] = count as u8;
  Rsc7Resource::from_pages(43, &data, &[])?.encode()
}

pub(in crate::core::format::ybn) fn compact_polygon_materials(geometry: &mut Geometry) {
  let source_materials = geometry.materials.clone();
  let mut materials: Vec<Material> = Vec::new();
  for polygon in &mut geometry.polygons {
    let old_index = polygon.material() as usize;
    let material = source_materials.get(old_index).copied().unwrap_or_default();
    let new_index = match materials.iter().position(|existing| *existing == material) {
      Some(index) => index,
      None => {
        materials.push(material);
        materials.len() - 1
      }
    } as u8;
    polygon.set_material_index(new_index);
  }
  geometry.materials = materials;
}

pub(in crate::core::format::ybn) fn encode_bound(
  bound: &Bound,
  data: &mut Vec<u8>,
  offset: usize,
) -> io::Result<()> {
  if bound.kind == "None" {
    return Ok(());
  }
  let normalized = quantized_geometry_bound(bound)?;
  let bound = normalized.as_ref().unwrap_or(bound);
  let block_size = size_for_bound(bound);
  ensure_len(data, offset + block_size);
  let mut common = bound.common.clone();
  if bound.kind == "GeometryBVH"
    && let Some(geometry) = &bound.geometry
    && !geometry.polygons.is_empty()
  {
    let (minimum, maximum) = bvh::geometry_polygon_bounds(geometry, bound)?;
    let center = std::array::from_fn(|axis| (minimum[axis] + maximum[axis]) * 0.5);
    write_vec3(&mut common, 48, minimum)?;
    write_vec3(&mut common, 32, maximum)?;
    write_vec3(&mut common, 64, center)?;
    write_vec3(&mut common, 80, center)?;
    let radius = (0..3).map(|axis| (maximum[axis] - center[axis]).powi(2)).sum::<f32>().sqrt();
    write_f32(&mut common, 20, radius)?;
  }
  data[offset..offset + 112].copy_from_slice(&common[..112]);
  let quantum = match &bound.geometry {
    Some(geometry) => match geometry.vertex_quantum {
      Some(quantum) => quantum,
      None if bound.kind == "GeometryBVH" => geometry_bvh_quantum(geometry, &common)?,
      None => geometry_quantum(&common)?,
    },
    None => [0.0; 3],
  };
  let extension_end = offset + 112 + bound.extension.len();
  if extension_end <= offset + block_size {
    data[offset + 112..extension_end].copy_from_slice(&bound.extension);
  }
  if bound.kind == "Composite" {
    let mut child_offsets = Vec::new();
    for child in &bound.children {
      if child.kind == "None" {
        child_offsets.push(None);
        continue;
      }
      let child_offset = reserve(data, size_for_bound(child))?;
      encode_bound(child, data, child_offset)?;
      child_offsets.push(Some(child_offset));
    }
    let mut pointers = Vec::new();
    for child_offset in &child_offsets {
      let pointer = child_offset.map(address).transpose()?.unwrap_or(0);
      pointers.extend_from_slice(&pointer.to_le_bytes());
    }
    let pointers_at = append(data, &pointers)?;
    let mut matrices = Vec::new();
    for matrix in &bound.transforms {
      write_matrix_record(&mut matrices, matrix);
    }
    let matrices_at = append(data, &matrices)?;
    let mut flags1 = Vec::new();
    let mut flags2 = Vec::new();
    for flags in &bound.flags {
      flags1.extend_from_slice(&flags[0].to_le_bytes());
      flags1.extend_from_slice(&flags[1].to_le_bytes());
      flags2.extend_from_slice(&flags[0].to_le_bytes());
      flags2.extend_from_slice(&flags[1].to_le_bytes());
    }
    let flags1_at = append(data, &flags1)?;
    let flags2_at = append(data, &flags2)?;
    put_u64(data, offset + 112, pointer_if_nonempty(pointers_at, pointers.len())?)?;
    put_u64(data, offset + 120, pointer_if_nonempty(matrices_at, matrices.len())?)?;
    put_u64(data, offset + 144, pointer_if_nonempty(flags1_at, flags1.len())?)?;
    put_u64(data, offset + 152, pointer_if_nonempty(flags2_at, flags2.len())?)?;
    put_u16(data, offset + 160, bound.children.len() as u16)?;
    put_u16(data, offset + 162, bound.children.len() as u16)?;
    put_u64(data, offset + 168, 0)?;
  } else if let Some(geometry) = &bound.geometry {
    if bound.kind == "GeometryBVH" {
      let mut geometry = geometry.clone();
      let pointer = bvh::append_geometry_bvh(data, &mut geometry, bound)?;
      compact_polygon_materials(&mut geometry);
      encode_geometry(&geometry, bound, quantum, data, offset)?;
      put_u64(data, offset + 304, pointer)?;
    } else {
      encode_geometry(geometry, bound, quantum, data, offset)?;
    }
  }
  Ok(())
}

/// Derive bounds and acceleration data from the vertices actually stored, not
/// their pre-quantized positions. Reusing the supplied grid prevents rebuild drift.
pub(in crate::core::format::ybn) fn quantized_geometry_bound(
  bound: &Bound
) -> io::Result<Option<Bound>> {
  let Some(geometry) = &bound.geometry else {
    return Ok(None);
  };
  let quantum = match geometry.vertex_quantum {
    Some(quantum) => quantum,
    None if bound.kind == "GeometryBVH" => geometry_bvh_quantum(geometry, &bound.common)?,
    None => geometry_quantum(&bound.common)?,
  };
  if quantum.iter().any(|value| !value.is_finite() || *value < 0.0) {
    return Err(invalid("YBN vertex quantum must be finite and nonnegative"));
  }
  let mut normalized = bound.clone();
  let output = normalized.geometry.as_mut().unwrap();
  for vertex in &mut output.vertices {
    for axis in 0..3 {
      if !vertex[axis].is_finite() {
        return Err(invalid("YBN contains a non-finite vertex"));
      }
      let scaled = if quantum[axis] == 0.0 { 0.0 } else { vertex[axis] / quantum[axis] };
      let packed = if geometry.vertex_quantum.is_some() { scaled.round() } else { scaled } as i16;
      vertex[axis] = packed as f32 * quantum[axis];
    }
  }
  output.vertex_quantum = Some(quantum);
  Ok(Some(normalized))
}

pub(in crate::core::format::ybn) fn geometry_quantum(common: &[u8]) -> io::Result<[f32; 3]> {
  let minimum = read_vec3(common, 48)?;
  let maximum = read_vec3(common, 32)?;
  Ok(std::array::from_fn(|axis| (maximum[axis] - minimum[axis]) * 0.5 / 32767.0))
}

pub(in crate::core::format::ybn) fn geometry_bvh_quantum(
  geometry: &Geometry,
  common: &[u8],
) -> io::Result<[f32; 3]> {
  let margin = read_f32(common, 44)?;
  if !margin.is_finite() || margin < 0.0 {
    return Err(invalid("GeometryBVH margin must be finite and nonnegative"));
  }
  let mut maximum = [0.0f32; 3];
  for vertex in &geometry.vertices {
    for axis in 0..3 {
      if !vertex[axis].is_finite() {
        return Err(invalid("GeometryBVH contains a non-finite vertex"));
      }
      maximum[axis] = maximum[axis].max(vertex[axis].abs());
    }
  }
  Ok(std::array::from_fn(|axis| {
    let extent = maximum[axis] + margin;
    if extent <= f32::EPSILON { 1.0 / 32767.0 } else { extent / 32767.0 }
  }))
}

pub(in crate::core::format::ybn) fn encode_geometry(
  geometry: &Geometry,
  bound: &Bound,
  quantum: [f32; 3],
  data: &mut Vec<u8>,
  offset: usize,
) -> io::Result<()> {
  let mut vertices = Vec::new();
  for vertex in &geometry.vertices {
    for axis in 0..3 {
      let scaled = if quantum[axis] == 0.0 { 0.0 } else { vertex[axis] / quantum[axis] };
      let quantized = if geometry.vertex_quantum.is_some() { scaled.round() } else { scaled };
      vertices.extend_from_slice(&(quantized as i16).to_le_bytes());
    }
  }
  let vertices_at = append(data, &vertices)?;
  let mut colors = Vec::new();
  for color in &geometry.vertex_colours {
    colors.extend_from_slice(color);
  }
  let colors_at = append(data, &colors)?;
  let mut materials = Vec::new();
  for material in &geometry.materials {
    let data1 = material.kind as u32
      | ((material.procedural_id as u32) << 8)
      | (((material.room_id & 0x1f) as u32) << 16)
      | (((material.ped_density & 7) as u32) << 21)
      | ((material.flags as u32 & 0xff) << 24);
    let data2 = ((material.flags as u32 >> 8) & 0xff)
      | ((material.colour_index as u32) << 8)
      | ((material.unknown as u32) << 16);
    materials.extend_from_slice(&data1.to_le_bytes());
    materials.extend_from_slice(&data2.to_le_bytes());
  }
  let materials_at = append(data, &materials)?;
  let mut material_colours = Vec::new();
  for colour in &geometry.material_colours {
    material_colours.extend_from_slice(colour);
  }
  let material_colours_at = append(data, &material_colours)?;
  let mut polygons = Vec::new();
  let mut material_indices = Vec::new();
  for polygon in &geometry.polygons {
    match polygon {
      Polygon::Triangle(triangle) => {
        let mut area = triangle.area.to_le_bytes();
        area[0] &= 0xf8;
        polygons.extend_from_slice(&area);
        for i in 0..3 {
          let value = triangle.vertices[i] | if triangle.vertex_flags[i] { 0x8000 } else { 0 };
          polygons.extend_from_slice(&value.to_le_bytes());
        }
        for edge_index in triangle.edge_indices {
          polygons.extend_from_slice(&edge_index.to_le_bytes());
        }
        material_indices.push(triangle.material);
      }
      Polygon::Box {
        material,
        vertices,
      } => {
        polygons.extend_from_slice(&3u32.to_le_bytes());
        for vertex in vertices {
          polygons.extend_from_slice(&vertex.to_le_bytes());
        }
        polygons.extend_from_slice(&0u32.to_le_bytes());
        material_indices.push(*material);
      }
      Polygon::Sphere {
        material,
        vertex,
        radius,
      } => {
        let record_start = polygons.len();
        polygons.extend_from_slice(&0u16.to_le_bytes());
        polygons.extend_from_slice(&vertex.to_le_bytes());
        polygons.extend_from_slice(&radius.to_le_bytes());
        polygons.extend_from_slice(&[0; 8]);
        polygons[record_start] |= 1;
        material_indices.push(*material);
      }
      Polygon::Capsule {
        material,
        vertex1,
        vertex2,
        radius,
      }
      | Polygon::Cylinder {
        material,
        vertex1,
        vertex2,
        radius,
      } => {
        let record_start = polygons.len();
        let tag = if matches!(polygon, Polygon::Capsule { .. }) { 2 } else { 4 };
        polygons.extend_from_slice(&0u16.to_le_bytes());
        polygons.extend_from_slice(&vertex1.to_le_bytes());
        polygons.extend_from_slice(&radius.to_le_bytes());
        polygons.extend_from_slice(&vertex2.to_le_bytes());
        polygons.extend_from_slice(&0u16.to_le_bytes());
        polygons.extend_from_slice(&0u32.to_le_bytes());
        polygons[record_start] |= tag;
        material_indices.push(*material);
      }
      Polygon::Unsupported {
        raw,
      } => {
        polygons.extend_from_slice(raw);
        material_indices.push(0);
      }
    }
  }
  let polygons_at = append(data, &polygons)?;
  let indices_at = append(data, &material_indices)?;
  put_u64(data, offset + 120, 0)?;
  put_u32(data, offset + 132, geometry.vertices.len() as u32)?;
  put_u64(data, offset + 136, pointer_if_nonempty(polygons_at, polygons.len())?)?;
  write_vec3(data, offset + 144, quantum)?;
  write_f32(data, offset + 156, geometry.unknown_9c)?;
  write_vec3(data, offset + 160, geometry.center)?;
  write_f32(data, offset + 172, geometry.unknown_ac)?;
  put_u64(data, offset + 176, pointer_if_nonempty(vertices_at, vertices.len())?)?;
  put_u64(data, offset + 184, pointer_if_nonempty(colors_at, colors.len())?)?;
  put_u32(data, offset + 208, geometry.vertices.len() as u32)?;
  put_u32(data, offset + 212, geometry.polygons.len() as u32)?;
  put_u64(data, offset + 240, pointer_if_nonempty(materials_at, materials.len())?)?;
  put_u64(data, offset + 248, pointer_if_nonempty(material_colours_at, material_colours.len())?)?;
  put_u64(data, offset + 280, pointer_if_nonempty(indices_at, material_indices.len())?)?;
  data[offset + 288] = geometry.materials.len() as u8;
  data[offset + 289] = geometry.material_colours.len() as u8;
  if bound.kind == "GeometryBVH" {
    put_u16(data, offset + 320, 0xffff)?;
  }
  Ok(())
}
