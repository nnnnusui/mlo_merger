//! Existing child/polygon merge implementation retained for compatibility.

use super::super::*;

/// Applies composite-child additions and removals relative to a vanilla YBN baseline.
pub fn merge_ybn_deltas(
  vanilla: &[u8],
  mods: &[&[u8]],
) -> io::Result<Vec<u8>> {
  let mut merged = read_ybn_root(vanilla)?;
  if merged.kind != "Composite" {
    return Err(invalid("YBN delta merge requires a Composite vanilla root"));
  }
  let baseline_keys = merged
    .children
    .iter()
    .map(
      |child| {
        if is_polygon_geometry(child) { Ok(None) } else { bound_identity(child).map(Some) }
      },
    )
    .collect::<io::Result<Vec<_>>>()?;
  let baseline_counts = counts(&baseline_keys.iter().flatten().cloned().collect::<Vec<_>>());
  let baseline_polygons = geometry_polygon_records(&merged)?;
  let mut baseline_polygon_lookup = PolygonLookup::default();
  for record in &baseline_polygons {
    baseline_polygon_lookup.insert(record.identity.clone());
  }
  let mut removed = HashSet::new();
  let mut added = HashMap::<Vec<u8>, (Bound, [f32; 16], [u32; 2])>::new();
  let mut removed_polygons = HashSet::<(usize, usize)>::new();
  let mut added_polygon_lookup = PolygonLookup::default();
  let mut added_geometry_children = Vec::<(Bound, [f32; 16], [u32; 2])>::new();
  let mut minimum = [f32::INFINITY; 3];
  let mut maximum = [f32::NEG_INFINITY; 3];
  let mut spheres = Vec::<([f32; 3], f32)>::new();

  for root in std::iter::once(read_ybn_root(vanilla)?)
    .chain(mods.iter().map(|bytes| read_ybn_root(bytes)).collect::<io::Result<Vec<_>>>()?)
  {
    if root.kind != "Composite" {
      return Err(invalid("YBN delta merge requires Composite roots"));
    }
    let root_minimum = read_vec3(&root.common, 48)?;
    let root_maximum = read_vec3(&root.common, 32)?;
    let sphere_center = read_vec3(&root.common, 80)?;
    let sphere_radius = read_f32(&root.common, 20)?;
    if root_minimum.iter().chain(&root_maximum).any(|value| !value.is_finite())
      || !sphere_radius.is_finite()
      || sphere_radius < 0.0
    {
      return Err(invalid("YBN root bounds are not finite"));
    }
    for axis in 0..3 {
      minimum[axis] = minimum[axis].min(root_minimum[axis]);
      maximum[axis] = maximum[axis].max(root_maximum[axis]);
    }
    spheres.push((sphere_center, sphere_radius));

    if spheres.len() > 1 {
      let mut remaining = baseline_counts.clone();
      let mod_polygons = geometry_polygon_records(&root)?;
      let mut mod_polygon_lookup = PolygonLookup::default();
      for record in &mod_polygons {
        mod_polygon_lookup.insert(record.identity.clone());
      }
      removed_polygons.extend(
        baseline_polygons
          .iter()
          .filter(|record| !mod_polygon_lookup.contains(&record.identity))
          .map(|record| (record.child_index, record.polygon_index)),
      );
      let mut additions_by_child = HashMap::<usize, HashSet<usize>>::new();
      for record in &mod_polygons {
        if !baseline_polygon_lookup.contains(&record.identity)
          && !added_polygon_lookup.contains(&record.identity)
        {
          added_polygon_lookup.insert(record.identity.clone());
          additions_by_child.entry(record.child_index).or_default().insert(record.polygon_index);
        }
      }
      for (index, child) in root.children.iter().enumerate() {
        if is_polygon_geometry(child) {
          if let Some(additions) = additions_by_child.get(&index)
            && !additions.is_empty()
          {
            added_geometry_children.push((
              geometry_polygon_subset(child, additions)?,
              root.transforms[index],
              root.flags[index],
            ));
          }
        } else {
          let key = bound_identity(child)?;
          if let Some(count) = remaining.get_mut(&key).filter(|count| **count > 0) {
            *count -= 1;
          } else {
            added
              .entry(key)
              .or_insert_with(|| (child.clone(), root.transforms[index], root.flags[index]));
          }
        }
      }
      removed.extend(remaining.into_iter().filter_map(|(key, count)| (count > 0).then_some(key)));
    }
  }

  let mut children = Vec::new();
  let mut transforms = Vec::new();
  let mut flags = Vec::new();
  for (index, child) in merged.children.drain(..).enumerate() {
    if let Some(key) = &baseline_keys[index]
      && removed.contains(key)
    {
      continue;
    }
    if is_polygon_geometry(&child) {
      let selected = child
        .geometry
        .as_ref()
        .expect("polygon geometry was checked")
        .polygons
        .iter()
        .enumerate()
        .filter_map(|(polygon_index, _)| {
          (!removed_polygons.contains(&(index, polygon_index))).then_some(polygon_index)
        })
        .collect::<HashSet<_>>();
      let child = geometry_polygon_subset(&child, &selected)?;
      if child.geometry.as_ref().is_some_and(|geometry| geometry.polygons.is_empty()) {
        continue;
      }
      children.push(child);
      transforms.push(merged.transforms[index]);
      flags.push(merged.flags[index]);
    } else {
      children.push(child);
      transforms.push(merged.transforms[index]);
      flags.push(merged.flags[index]);
    }
  }
  for (key, (child, transform, child_flags)) in added {
    if !baseline_counts.contains_key(&key) {
      children.push(child);
      transforms.push(transform);
      flags.push(child_flags);
    }
  }
  for (child, transform, child_flags) in added_geometry_children {
    children.push(child);
    transforms.push(transform);
    flags.push(child_flags);
  }
  merged.children = children;
  merged.transforms = transforms;
  merged.flags = flags;
  if merged.children.len() > u16::MAX as usize {
    return Err(invalid("merged YBN composite has too many children"));
  }

  let center = std::array::from_fn(|axis| (minimum[axis] + maximum[axis]) * 0.5);
  let radius = spheres
    .iter()
    .map(|(sphere_center, sphere_radius)| {
      (0..3).map(|axis| (sphere_center[axis] - center[axis]).powi(2)).sum::<f32>().sqrt()
        + sphere_radius
    })
    .fold(0.0f32, f32::max);
  write_vec3(&mut merged.common, 48, minimum)?;
  write_vec3(&mut merged.common, 32, maximum)?;
  write_vec3(&mut merged.common, 64, center)?;
  write_vec3(&mut merged.common, 80, center)?;
  write_f32(&mut merged.common, 20, radius)?;
  encode_ybn_bound(&merged)
}

pub(in crate::core::format::ybn) fn counts(keys: &[Vec<u8>]) -> HashMap<Vec<u8>, usize> {
  let mut counts = HashMap::new();
  for key in keys {
    *counts.entry(key.clone()).or_insert(0) += 1;
  }
  counts
}

pub(in crate::core::format::ybn) fn is_polygon_geometry(bound: &Bound) -> bool {
  bound.geometry.as_ref().is_some_and(|geometry| {
    !geometry.polygons.is_empty()
      && geometry.polygons.iter().all(|polygon| !matches!(polygon, Polygon::Unsupported { .. }))
  })
}

#[derive(Clone)]
pub(in crate::core::format::ybn) struct GeometryPolygonRecord {
  child_index: usize,
  polygon_index: usize,
  pub(in crate::core::format::ybn) identity: PolygonIdentity,
}

#[derive(Clone)]
pub(in crate::core::format::ybn) struct PolygonIdentity {
  kind: u8,
  material: Material,
  vertices: Vec<[f32; 3]>,
  vertex_flags: Vec<bool>,
  radius: Option<f32>,
}

impl PolygonIdentity {
  fn signature(&self) -> Vec<u8> {
    let mut signature = vec![self.kind];
    signature.extend_from_slice(&[
      self.material.kind,
      self.material.procedural_id,
      self.material.room_id,
      self.material.ped_density,
    ]);
    signature.extend_from_slice(&self.material.flags.to_le_bytes());
    signature.push(self.material.colour_index);
    signature.extend_from_slice(&self.material.unknown.to_le_bytes());
    signature.push(self.vertices.len() as u8);
    let mut flags = self.vertex_flags.clone();
    flags.sort_unstable();
    signature.extend(flags.into_iter().map(u8::from));
    signature
  }

  fn centroid(&self) -> [f32; 3] {
    std::array::from_fn(|axis| {
      self.vertices.iter().map(|vertex| vertex[axis]).sum::<f32>() / self.vertices.len() as f32
    })
  }

  pub(in crate::core::format::ybn) fn matches(
    &self,
    other: &Self,
  ) -> bool {
    if self.kind != other.kind
      || self.material != other.material
      || self.vertices.len() != other.vertices.len()
      || self.vertex_flags.len() != other.vertex_flags.len()
      || !matches_radius(self.radius, other.radius)
    {
      return false;
    }

    if self.kind == 0 {
      return (0..self.vertices.len()).any(|shift| {
        (0..self.vertices.len()).all(|index| {
          points_match(self.vertices[index], other.vertices[(index + shift) % self.vertices.len()])
            && self.vertex_flags[index]
              == other.vertex_flags[(index + shift) % self.vertex_flags.len()]
        })
      });
    }

    self.vertices.iter().zip(&other.vertices).all(|(first, second)| points_match(*first, *second))
  }
}

#[derive(Default)]
pub(in crate::core::format::ybn) struct PolygonLookup {
  buckets: HashMap<(Vec<u8>, [i64; 3]), Vec<PolygonIdentity>>,
}

impl PolygonLookup {
  pub(in crate::core::format::ybn) fn insert(
    &mut self,
    identity: PolygonIdentity,
  ) {
    let key = polygon_bucket_key(&identity);
    self.buckets.entry(key).or_default().push(identity);
  }

  pub(in crate::core::format::ybn) fn contains(
    &self,
    identity: &PolygonIdentity,
  ) -> bool {
    let (signature, cell) = polygon_bucket_key(identity);
    for x in -1..=1 {
      for y in -1..=1 {
        for z in -1..=1 {
          let key = (signature.clone(), [cell[0] + x, cell[1] + y, cell[2] + z]);
          if self.buckets.get(&key).is_some_and(|candidates| {
            candidates.iter().any(|candidate| identity.matches(candidate))
          }) {
            return true;
          }
        }
      }
    }
    false
  }
}

pub(in crate::core::format::ybn) fn polygon_bucket_key(
  identity: &PolygonIdentity
) -> (Vec<u8>, [i64; 3]) {
  let centroid = identity.centroid();
  let bucket_width = f64::from(YBN_POLYGON_MATCH_TOLERANCE) * 2.0;
  (identity.signature(), centroid.map(|value| (f64::from(value) / bucket_width).floor() as i64))
}

pub(in crate::core::format::ybn) fn points_match(
  first: [f32; 3],
  second: [f32; 3],
) -> bool {
  (0..3).all(|axis| (first[axis] - second[axis]).abs() <= YBN_POLYGON_MATCH_TOLERANCE)
}

pub(in crate::core::format::ybn) fn matches_radius(
  first: Option<f32>,
  second: Option<f32>,
) -> bool {
  match (first, second) {
    (Some(first), Some(second)) => (first - second).abs() <= YBN_POLYGON_MATCH_TOLERANCE,
    (None, None) => true,
    _ => false,
  }
}

pub(in crate::core::format::ybn) fn geometry_polygon_records(
  root: &Bound
) -> io::Result<Vec<GeometryPolygonRecord>> {
  let mut records = Vec::new();
  for (child_index, child) in root.children.iter().enumerate() {
    if !is_polygon_geometry(child) {
      continue;
    }
    let geometry = child.geometry.as_ref().ok_or_else(|| invalid("YBN geometry is missing"))?;
    for (polygon_index, polygon) in geometry.polygons.iter().enumerate() {
      records.push(GeometryPolygonRecord {
        child_index,
        polygon_index,
        identity: polygon_identity(child, geometry, polygon)?,
      });
    }
  }
  Ok(records)
}

pub(in crate::core::format::ybn) fn polygon_identity(
  bound: &Bound,
  geometry: &Geometry,
  polygon: &Polygon,
) -> io::Result<PolygonIdentity> {
  let material = geometry.materials.get(polygon.material() as usize).copied().unwrap_or_default();
  let mut identity = PolygonIdentity {
    kind: 0,
    material,
    vertices: Vec::new(),
    vertex_flags: Vec::new(),
    radius: None,
  };
  let mut add_vertex = |index: u16| -> io::Result<()> {
    let vertex = geometry
      .vertices
      .get(index as usize)
      .ok_or_else(|| invalid("polygon vertex index is out of range"))?;
    let local = std::array::from_fn(|axis| vertex[axis] + geometry.center[axis]);
    let world = transform_ybn_point(local, bound.transform);
    if world.iter().any(|value| !value.is_finite()) {
      return Err(invalid("polygon contains a non-finite vertex"));
    }
    identity.vertices.push(world);
    Ok(())
  };
  match polygon {
    Polygon::Triangle(triangle) => {
      identity.kind = 0;
      identity.vertex_flags = triangle.vertex_flags.to_vec();
      for vertex in triangle.vertices {
        add_vertex(vertex)?;
      }
    }
    Polygon::Box {
      vertices,
      ..
    } => {
      identity.kind = 1;
      for vertex in vertices {
        add_vertex(*vertex)?;
      }
      sort_polygon_vertices(&mut identity.vertices);
    }
    Polygon::Sphere {
      vertex,
      radius,
      ..
    } => {
      identity.kind = 2;
      identity.radius = Some(*radius);
      add_vertex(*vertex)?;
    }
    Polygon::Capsule {
      vertex1,
      vertex2,
      radius,
      ..
    } => {
      identity.kind = 3;
      identity.radius = Some(*radius);
      add_vertex(*vertex1)?;
      add_vertex(*vertex2)?;
      sort_polygon_vertices(&mut identity.vertices);
    }
    Polygon::Cylinder {
      vertex1,
      vertex2,
      radius,
      ..
    } => {
      identity.kind = 4;
      identity.radius = Some(*radius);
      add_vertex(*vertex1)?;
      add_vertex(*vertex2)?;
      sort_polygon_vertices(&mut identity.vertices);
    }
    Polygon::Unsupported {
      ..
    } => return Err(invalid("unsupported YBN polygon cannot be diffed")),
  }
  if identity.radius.is_some_and(|radius| !radius.is_finite()) {
    return Err(invalid("polygon radius is not finite"));
  }
  Ok(identity)
}

pub(in crate::core::format::ybn) fn sort_polygon_vertices(vertices: &mut [[f32; 3]]) {
  vertices.sort_by(|first, second| {
    first[0]
      .total_cmp(&second[0])
      .then_with(|| first[1].total_cmp(&second[1]))
      .then_with(|| first[2].total_cmp(&second[2]))
  });
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
