use std::{
  collections::{HashMap, HashSet},
  io,
};

use super::{
  resource_file::Rsc7Resource,
  xml_tree::{XmlElement, parse_xml, write_text_content},
};

mod bvh;

const BASE: u64 = 0x5000_0000;
const ROOT_OFFSET: usize = 0;
const BOUNDS_SIZE: usize = 112;
const GEOMETRY_SIZE: usize = 304;
const GEOMETRY_BVH_SIZE: usize = 336;
const COMPOSITE_SIZE: usize = 176;
const YBN_POLYGON_MATCH_TOLERANCE: f32 = 0.005;

#[derive(Clone)]
struct Bound {
  kind: String,
  common: Vec<u8>,
  extension: Vec<u8>,
  geometry: Option<Geometry>,
  children: Vec<Bound>,
  transform: Option<[f32; 16]>,
  composite_flags: [u32; 2],
  transforms: Vec<[f32; 16]>,
  flags: Vec<[u32; 2]>,
}

#[derive(Clone, Default)]
struct Geometry {
  center: [f32; 3],
  unknown_9c: f32,
  unknown_ac: f32,
  vertex_quantum: Option<[f32; 3]>,
  vertices: Vec<[f32; 3]>,
  materials: Vec<Material>,
  material_colours: Vec<[u8; 4]>,
  vertex_colours: Vec<[u8; 4]>,
  polygons: Vec<Polygon>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Material {
  kind: u8,
  procedural_id: u8,
  room_id: u8,
  ped_density: u8,
  flags: u16,
  colour_index: u8,
  unknown: u16,
}

#[derive(Clone, Copy, Default)]
struct Triangle {
  material: u8,
  area: f32,
  vertices: [u16; 3],
  vertex_flags: [bool; 3],
  edge_indices: [u16; 3],
}
#[derive(Clone, Copy)]
enum Polygon {
  Triangle(Triangle),
  Box {
    material: u8,
    vertices: [u16; 4],
  },
  Sphere {
    material: u8,
    vertex: u16,
    radius: f32,
  },
  Capsule {
    material: u8,
    vertex1: u16,
    vertex2: u16,
    radius: f32,
  },
  Cylinder {
    material: u8,
    vertex1: u16,
    vertex2: u16,
    radius: f32,
  },
  Unsupported {
    raw: [u8; 16],
  },
}

impl Polygon {
  fn material(&self) -> u8 {
    match self {
      Self::Triangle(polygon) => polygon.material,
      Self::Box {
        material,
        ..
      }
      | Self::Sphere {
        material,
        ..
      }
      | Self::Capsule {
        material,
        ..
      }
      | Self::Cylinder {
        material,
        ..
      } => *material,
      Self::Unsupported {
        ..
      } => 0,
    }
  }

  fn set_material_index(
    &mut self,
    index: u8,
  ) {
    match self {
      Self::Triangle(polygon) => polygon.material = index,
      Self::Box {
        material,
        ..
      }
      | Self::Sphere {
        material,
        ..
      }
      | Self::Capsule {
        material,
        ..
      }
      | Self::Cylinder {
        material,
        ..
      } => *material = index,
      Self::Unsupported {
        ..
      } => {}
    }
  }
}

/// Converts an RSC7 YBN Bounds resource to CodeWalker-style XML.
pub fn ybn_to_xml(bytes: &[u8]) -> io::Result<String> {
  let resource = Rsc7Resource::decode(bytes)?;
  let root = read_bound(&resource, BASE + ROOT_OFFSET as u64, None)?;
  let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<BoundsFile>\n");
  write_bound_xml(&root, 1, "Bounds", &mut xml);
  xml.push_str("</BoundsFile>\n");
  Ok(xml)
}

/// Rebuilds a Native RSC7 YBN from CodeWalker Bounds XML.
pub fn xml_to_ybn(xml: &str) -> io::Result<Vec<u8>> {
  let root = parse_xml(xml)?;
  if root.name != "BoundsFile" {
    return Err(invalid("YBN XML root must be BoundsFile"));
  }
  let bound_node = child(&root, "Bounds")?;
  let bound = read_bound_xml(bound_node, None)?;
  encode_ybn_bound(&bound)
}

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

fn read_ybn_root(bytes: &[u8]) -> io::Result<Bound> {
  let resource = Rsc7Resource::decode(bytes)?;
  read_bound(&resource, BASE + ROOT_OFFSET as u64, None)
}

fn counts(keys: &[Vec<u8>]) -> HashMap<Vec<u8>, usize> {
  let mut counts = HashMap::new();
  for key in keys {
    *counts.entry(key.clone()).or_insert(0) += 1;
  }
  counts
}

fn is_polygon_geometry(bound: &Bound) -> bool {
  bound.geometry.as_ref().is_some_and(|geometry| {
    !geometry.polygons.is_empty()
      && geometry.polygons.iter().all(|polygon| !matches!(polygon, Polygon::Unsupported { .. }))
  })
}

#[derive(Clone)]
struct GeometryPolygonRecord {
  child_index: usize,
  polygon_index: usize,
  identity: PolygonIdentity,
}

#[derive(Clone)]
struct PolygonIdentity {
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

  fn matches(
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
struct PolygonLookup {
  buckets: HashMap<(Vec<u8>, [i64; 3]), Vec<PolygonIdentity>>,
}

impl PolygonLookup {
  fn insert(
    &mut self,
    identity: PolygonIdentity,
  ) {
    let key = polygon_bucket_key(&identity);
    self.buckets.entry(key).or_default().push(identity);
  }

  fn contains(
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

fn polygon_bucket_key(identity: &PolygonIdentity) -> (Vec<u8>, [i64; 3]) {
  let centroid = identity.centroid();
  let bucket_width = f64::from(YBN_POLYGON_MATCH_TOLERANCE) * 2.0;
  (identity.signature(), centroid.map(|value| (f64::from(value) / bucket_width).floor() as i64))
}

fn points_match(
  first: [f32; 3],
  second: [f32; 3],
) -> bool {
  (0..3).all(|axis| (first[axis] - second[axis]).abs() <= YBN_POLYGON_MATCH_TOLERANCE)
}

fn matches_radius(
  first: Option<f32>,
  second: Option<f32>,
) -> bool {
  match (first, second) {
    (Some(first), Some(second)) => (first - second).abs() <= YBN_POLYGON_MATCH_TOLERANCE,
    (None, None) => true,
    _ => false,
  }
}

fn geometry_polygon_records(root: &Bound) -> io::Result<Vec<GeometryPolygonRecord>> {
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

fn polygon_identity(
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

fn sort_polygon_vertices(vertices: &mut [[f32; 3]]) {
  vertices.sort_by(|first, second| {
    first[0]
      .total_cmp(&second[0])
      .then_with(|| first[1].total_cmp(&second[1]))
      .then_with(|| first[2].total_cmp(&second[2]))
  });
}

fn transform_ybn_point(
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

fn geometry_polygon_subset(
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

fn bound_identity(bound: &Bound) -> io::Result<Vec<u8>> {
  let mut identity = Vec::new();
  append_bound_identity(bound, &mut identity)?;
  Ok(identity)
}

fn append_bound_identity(
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

fn append_quantized_float(
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

fn encode_ybn_bound(bound: &Bound) -> io::Result<Vec<u8>> {
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

fn read_bound(
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

fn read_geometry(
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

fn read_bound_xml(
  node: &XmlElement,
  transform: Option<[f32; 16]>,
) -> io::Result<Bound> {
  let kind =
    node.attributes.get("type").ok_or_else(|| invalid("YBN Bounds item has no type"))?.clone();
  if kind == "None" {
    return Ok(empty_bound(transform));
  }
  if !matches!(
    kind.as_str(),
    "Sphere"
      | "Capsule"
      | "Box"
      | "Geometry"
      | "GeometryBVH"
      | "Composite"
      | "Disc"
      | "Cylinder"
      | "Cloth"
  ) {
    return Err(invalid(&format!("Native YBN does not support Bounds type {kind}")));
  }
  let mut common = vec![0; BOUNDS_SIZE];
  common[16] = match kind.as_str() {
    "Sphere" => 0,
    "Capsule" => 1,
    "Box" => 3,
    "Geometry" => 4,
    "GeometryBVH" => 8,
    "Composite" => 10,
    "Disc" => 12,
    "Cylinder" => 13,
    "Cloth" => 15,
    _ => unreachable!(),
  };
  write_f32(&mut common, 20, number(node, "SphereRadius")?)?;
  write_vec3(&mut common, 32, vec3(child(node, "BoxMax")?)?)?;
  write_f32(&mut common, 44, number(node, "Margin")?)?;
  write_vec3(&mut common, 48, vec3(child(node, "BoxMin")?)?)?;
  put_u32(&mut common, 60, number(node, "UnkType")?)?;
  write_vec3(&mut common, 64, vec3(child(node, "BoxCenter")?)?)?;
  common[76] = number(node, "MaterialIndex")?;
  common[77] = number(node, "ProceduralID")?;
  common[78] =
    (number::<u8>(node, "RoomID")? & 0x1f) | ((number::<u8>(node, "PedDensity")? & 7) << 5);
  common[79] = number(node, "UnkFlags")?;
  write_vec3(&mut common, 80, vec3(child(node, "SphereCenter")?)?)?;
  common[92] = number(node, "PolyFlags")?;
  common[93] = number(node, "MaterialColourIndex")?;
  write_vec3(&mut common, 96, vec3(child(node, "Inertia")?)?)?;
  write_f32(&mut common, 108, number(node, "Volume")?)?;
  let extension_len = if matches!(kind.as_str(), "Capsule" | "Disc" | "Cylinder") { 16 } else { 0 };
  let mut bound = Bound {
    kind: kind.clone(),
    common,
    extension: vec![0; extension_len],
    geometry: None,
    children: Vec::new(),
    transform,
    composite_flags: [0; 2],
    transforms: Vec::new(),
    flags: Vec::new(),
  };
  if kind == "Composite" {
    if let Some(children) = node.children.iter().find(|child| child.name == "Children") {
      for child_node in children.children.iter().filter(|child| child.name == "Item") {
        if child_node.attributes.get("type").is_some_and(|kind| kind == "None") {
          bound.transforms.push(identity());
          bound.flags.push([0; 2]);
          bound.children.push(empty_bound(None));
          continue;
        }
        let transform = read_float_array(child_node, "CompositeTransform", 16)?;
        let flags1 = parse_composite_flags(&text(child(child_node, "CompositeFlags1")?))?;
        let flags2 = parse_composite_flags(&text(child(child_node, "CompositeFlags2")?))?;
        bound.transforms.push(transform);
        bound.flags.push([flags1, flags2]);
        let mut child_bound = read_bound_xml(child_node, Some(transform))?;
        child_bound.composite_flags = [flags1, flags2];
        bound.children.push(child_bound);
      }
    }
  } else if matches!(kind.as_str(), "Geometry" | "GeometryBVH") {
    bound.geometry = Some(geometry_from_xml(node)?);
  }
  Ok(bound)
}

fn geometry_from_xml(node: &XmlElement) -> io::Result<Geometry> {
  let mut geometry = Geometry {
    center: vec3(child(node, "GeometryCenter")?)?,
    unknown_9c: number(node, "UnkFloat1")?,
    unknown_ac: number(node, "UnkFloat2")?,
    ..Geometry::default()
  };
  if let Some(quantum) = node.children.iter().find(|child| child.name == "VertexQuantum") {
    let quantum = vec3(quantum)?;
    if quantum.iter().any(|value| !value.is_finite() || *value < 0.0) {
      return Err(invalid("YBN vertex quantum must be finite and nonnegative"));
    }
    geometry.vertex_quantum = Some(quantum);
  }
  if let Some(vertices) = node.children.iter().find(|child| child.name == "Vertices") {
    geometry.vertices = parse_vectors(&vertices.text)?;
  }
  if let Some(colors) = node.children.iter().find(|child| child.name == "Materials") {
    for item in colors.children.iter().filter(|child| child.name == "Item") {
      geometry.materials.push(Material {
        kind: number(item, "Type")?,
        procedural_id: number(item, "ProceduralID")?,
        room_id: number(item, "RoomID")?,
        ped_density: number(item, "PedDensity")?,
        flags: parse_material_flags(&text(child(item, "Flags")?))?,
        colour_index: number(item, "MaterialColourIndex")?,
        unknown: number(item, "Unk")?,
      });
    }
  }
  if let Some(colors) = node.children.iter().find(|child| child.name == "MaterialColours") {
    geometry.material_colours = parse_colors(&colors.text)?;
  }
  if let Some(colors) = node.children.iter().find(|child| child.name == "VertexColours") {
    geometry.vertex_colours = parse_colors(&colors.text)?;
  }
  if let Some(polygons) = node.children.iter().find(|child| child.name == "Polygons") {
    for polygon in &polygons.children {
      let material = polygon_attr(polygon, "m")?;
      match polygon.name.as_str() {
        "Triangle" => geometry.polygons.push(Polygon::Triangle(Triangle {
          material,
          area: 0.0,
          vertices: [
            polygon_attr(polygon, "v1")?,
            polygon_attr(polygon, "v2")?,
            polygon_attr(polygon, "v3")?,
          ],
          vertex_flags: [
            polygon_attr::<u8>(polygon, "f1")? != 0,
            polygon_attr::<u8>(polygon, "f2")? != 0,
            polygon_attr::<u8>(polygon, "f3")? != 0,
          ],
          edge_indices: [u16::MAX; 3],
        })),
        "Box" => geometry.polygons.push(Polygon::Box {
          material,
          vertices: [
            polygon_attr(polygon, "v1")?,
            polygon_attr(polygon, "v2")?,
            polygon_attr(polygon, "v3")?,
            polygon_attr(polygon, "v4")?,
          ],
        }),
        "Sphere" => geometry.polygons.push(Polygon::Sphere {
          material,
          vertex: polygon_attr(polygon, "v")?,
          radius: polygon_attr(polygon, "radius")?,
        }),
        "Capsule" => geometry.polygons.push(Polygon::Capsule {
          material,
          vertex1: polygon_attr(polygon, "v1")?,
          vertex2: polygon_attr(polygon, "v2")?,
          radius: polygon_attr(polygon, "radius")?,
        }),
        "Cylinder" => geometry.polygons.push(Polygon::Cylinder {
          material,
          vertex1: polygon_attr(polygon, "v1")?,
          vertex2: polygon_attr(polygon, "v2")?,
          radius: polygon_attr(polygon, "radius")?,
        }),
        _ => {
          return Err(invalid(&format!(
            "Native YBN polygon {} is not supported yet",
            polygon.name
          )));
        }
      }
    }
  }
  compact_polygon_materials(&mut geometry);
  Ok(geometry)
}

fn compact_polygon_materials(geometry: &mut Geometry) {
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

fn encode_bound(
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

/// Makes bounds, triangle metadata and BVH use the exact vertices written to the resource.
fn quantized_geometry_bound(bound: &Bound) -> io::Result<Option<Bound>> {
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

fn geometry_quantum(common: &[u8]) -> io::Result<[f32; 3]> {
  let minimum = read_vec3(common, 48)?;
  let maximum = read_vec3(common, 32)?;
  Ok(std::array::from_fn(|axis| (maximum[axis] - minimum[axis]) * 0.5 / 32767.0))
}

fn geometry_bvh_quantum(
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

fn encode_geometry(
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

fn write_bound_xml(
  bound: &Bound,
  depth: usize,
  item_tag: &str,
  xml: &mut String,
) {
  if bound.kind == "None" {
    indent(xml, depth);
    xml.push_str(&format!("<{item_tag} type=\"None\" />\n"));
    return;
  }
  indent(xml, depth);
  xml.push_str(&format!("<{item_tag} type=\"{}\">\n", bound.kind));
  let common = &bound.common;
  vec_tag(xml, depth + 1, "BoxMin", read_vec3(common, 48).unwrap());
  vec_tag(xml, depth + 1, "BoxMax", read_vec3(common, 32).unwrap());
  vec_tag(xml, depth + 1, "BoxCenter", read_vec3(common, 64).unwrap());
  vec_tag(xml, depth + 1, "SphereCenter", read_vec3(common, 80).unwrap());
  val_tag(xml, depth + 1, "SphereRadius", read_f32(common, 20).unwrap());
  val_tag(xml, depth + 1, "Margin", read_f32(common, 44).unwrap());
  val_tag(xml, depth + 1, "Volume", read_f32(common, 108).unwrap());
  vec_tag(xml, depth + 1, "Inertia", read_vec3(common, 96).unwrap());
  val_tag(xml, depth + 1, "MaterialIndex", common[76]);
  val_tag(xml, depth + 1, "MaterialColourIndex", common[93]);
  val_tag(xml, depth + 1, "ProceduralID", common[77]);
  val_tag(xml, depth + 1, "RoomID", common[78] & 0x1f);
  val_tag(xml, depth + 1, "PedDensity", common[78] >> 5);
  val_tag(xml, depth + 1, "UnkFlags", common[79]);
  val_tag(xml, depth + 1, "PolyFlags", common[92]);
  val_tag(xml, depth + 1, "UnkType", read_u32(common, 60).unwrap());
  if let Some(transform) = bound.transform {
    text_tag(
      xml,
      depth + 1,
      "CompositeTransform",
      &transform.iter().map(ToString::to_string).collect::<Vec<_>>().join(" "),
    );
    text_tag(xml, depth + 1, "CompositeFlags1", &composite_flag_text(bound.composite_flags[0]));
    text_tag(xml, depth + 1, "CompositeFlags2", &composite_flag_text(bound.composite_flags[1]));
  }
  if let Some(geometry) = &bound.geometry {
    vec_tag(xml, depth + 1, "GeometryCenter", geometry.center);
    if let Some(quantum) = geometry.vertex_quantum {
      vec_tag(xml, depth + 1, "VertexQuantum", quantum);
    }
    val_tag(xml, depth + 1, "UnkFloat1", geometry.unknown_9c);
    val_tag(xml, depth + 1, "UnkFloat2", geometry.unknown_ac);
    write_array_open(xml, depth + 1, "Materials");
    for material in &geometry.materials {
      indent(xml, depth + 2);
      xml.push_str("<Item>\n");
      val_tag(xml, depth + 3, "Type", material.kind);
      val_tag(xml, depth + 3, "ProceduralID", material.procedural_id);
      val_tag(xml, depth + 3, "RoomID", material.room_id);
      val_tag(xml, depth + 3, "PedDensity", material.ped_density);
      text_tag(xml, depth + 3, "Flags", &material_flag_text(material.flags));
      val_tag(xml, depth + 3, "MaterialColourIndex", material.colour_index);
      val_tag(xml, depth + 3, "Unk", material.unknown);
      indent(xml, depth + 2);
      xml.push_str("</Item>\n");
    }
    write_array_close(xml, depth + 1, "Materials");
    if !geometry.material_colours.is_empty() {
      text_tag(xml, depth + 1, "MaterialColours", &format_colors(&geometry.material_colours));
    }
    if !geometry.vertices.is_empty() {
      text_tag(xml, depth + 1, "Vertices", &format_vectors(&geometry.vertices));
    }
    if !geometry.vertex_colours.is_empty() {
      text_tag(xml, depth + 1, "VertexColours", &format_colors(&geometry.vertex_colours));
    }
    if geometry.polygons.iter().any(|polygon| !matches!(polygon, Polygon::Unsupported { .. })) {
      write_array_open(xml, depth + 1, "Polygons");
      for polygon in &geometry.polygons {
        if matches!(polygon, Polygon::Unsupported { .. }) {
          continue;
        }
        indent(xml, depth + 2);
        match polygon {
          Polygon::Triangle(triangle) => xml.push_str(&format!(
            "<Triangle m=\"{}\" v1=\"{}\" v2=\"{}\" v3=\"{}\" f1=\"{}\" f2=\"{}\" f3=\"{}\" />\n",
            triangle.material,
            triangle.vertices[0],
            triangle.vertices[1],
            triangle.vertices[2],
            u8::from(triangle.vertex_flags[0]),
            u8::from(triangle.vertex_flags[1]),
            u8::from(triangle.vertex_flags[2])
          )),
          Polygon::Box {
            material,
            vertices,
          } => xml.push_str(&format!(
            "<Box m=\"{}\" v1=\"{}\" v2=\"{}\" v3=\"{}\" v4=\"{}\" />\n",
            material, vertices[0], vertices[1], vertices[2], vertices[3]
          )),
          Polygon::Sphere {
            material,
            vertex,
            radius,
          } => xml.push_str(&format!(
            "<Sphere m=\"{}\" v=\"{}\" radius=\"{}\" />\n",
            material, vertex, radius
          )),
          Polygon::Capsule {
            material,
            vertex1,
            vertex2,
            radius,
          } => xml.push_str(&format!(
            "<Capsule m=\"{}\" v1=\"{}\" v2=\"{}\" radius=\"{}\" />\n",
            material, vertex1, vertex2, radius
          )),
          Polygon::Cylinder {
            material,
            vertex1,
            vertex2,
            radius,
          } => xml.push_str(&format!(
            "<Cylinder m=\"{}\" v1=\"{}\" v2=\"{}\" radius=\"{}\" />\n",
            material, vertex1, vertex2, radius
          )),
          Polygon::Unsupported {
            ..
          } => unreachable!("unsupported polygons are omitted from XML"),
        }
      }
      write_array_close(xml, depth + 1, "Polygons");
    }
  }
  if !bound.children.is_empty() {
    write_array_open(xml, depth + 1, "Children");
    for child in &bound.children {
      write_bound_xml(child, depth + 2, "Item", xml);
    }
    write_array_close(xml, depth + 1, "Children");
  }
  indent(xml, depth);
  xml.push_str(&format!("</{item_tag}>\n"));
}

fn read_address_table(
  resource: &Rsc7Resource,
  pointer: u64,
  count: usize,
  stride: usize,
) -> io::Result<&[u8]> {
  if count == 0 {
    return Ok(&[]);
  }
  if pointer == 0 {
    return Err(invalid("YBN non-empty table has a null pointer"));
  }
  let size = count.checked_mul(stride).ok_or_else(|| invalid("YBN table size overflows"))?;
  resource.read_address(pointer, size)
}
fn read_optional_address_table(
  resource: &Rsc7Resource,
  pointer: u64,
  count: usize,
  stride: usize,
) -> io::Result<Vec<u8>> {
  if pointer == 0 || count == 0 {
    return Ok(Vec::new());
  }
  let size = count.checked_mul(stride).ok_or_else(|| invalid("YBN table size overflows"))?;
  let address =
    u32::try_from(pointer).map_err(|_| invalid("YBN optional table pointer is invalid"))?;
  let (data, offset) = match address & 0xf000_0000 {
    0x5000_0000 => (&resource.system_data, (address & 0x0fff_ffff) as usize),
    0x6000_0000 => (&resource.graphics_data, (address & 0x0fff_ffff) as usize),
    _ => return Err(invalid("YBN optional table pointer uses an unknown page region")),
  };
  let available = if offset >= data.len() { 0 } else { size.min(data.len() - offset) };
  let mut result = vec![0; size];
  if available > 0 {
    result[..available].copy_from_slice(&data[offset..offset + available]);
  }
  Ok(result)
}
fn read_pointer_array(
  resource: &Rsc7Resource,
  pointer: u64,
  count: usize,
) -> io::Result<Vec<u64>> {
  if count == 0 {
    return Ok(Vec::new());
  }
  let bytes = read_address_table(resource, pointer, count, 8)?;
  (0..count).map(|index| read_u64(bytes, index * 8)).collect()
}
fn bound_type(value: u8) -> io::Result<String> {
  match value {
    255 => Ok("None".into()),
    0 => Ok("Sphere".into()),
    1 => Ok("Capsule".into()),
    3 => Ok("Box".into()),
    4 => Ok("Geometry".into()),
    8 => Ok("GeometryBVH".into()),
    10 => Ok("Composite".into()),
    12 => Ok("Disc".into()),
    13 => Ok("Cylinder".into()),
    15 => Ok("Cloth".into()),
    _ => Err(invalid(&format!("Native YBN does not support Bounds type id {value}"))),
  }
}
fn size_for_bound(bound: &Bound) -> usize {
  match bound.kind.as_str() {
    "None" => 0,
    "Composite" => COMPOSITE_SIZE,
    "Geometry" => GEOMETRY_SIZE,
    "GeometryBVH" => GEOMETRY_BVH_SIZE,
    "Capsule" | "Disc" | "Cylinder" => 128,
    _ => BOUNDS_SIZE,
  }
}
fn empty_bound(transform: Option<[f32; 16]>) -> Bound {
  Bound {
    kind: "None".into(),
    common: vec![0; BOUNDS_SIZE],
    extension: Vec::new(),
    geometry: None,
    children: Vec::new(),
    transform,
    composite_flags: [0; 2],
    transforms: Vec::new(),
    flags: Vec::new(),
  }
}
fn read_u16(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u16> {
  Ok(u16::from_le_bytes(array(bytes, offset)?))
}
fn read_i16(
  bytes: &[u8],
  offset: usize,
) -> io::Result<i16> {
  Ok(i16::from_le_bytes(array(bytes, offset)?))
}
fn read_u32(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u32> {
  Ok(u32::from_le_bytes(array(bytes, offset)?))
}
fn read_u64(
  bytes: &[u8],
  offset: usize,
) -> io::Result<u64> {
  Ok(u64::from_le_bytes(array(bytes, offset)?))
}
fn read_f32(
  bytes: &[u8],
  offset: usize,
) -> io::Result<f32> {
  Ok(f32::from_le_bytes(array(bytes, offset)?))
}
fn read_vec3(
  bytes: &[u8],
  offset: usize,
) -> io::Result<[f32; 3]> {
  Ok([read_f32(bytes, offset)?, read_f32(bytes, offset + 4)?, read_f32(bytes, offset + 8)?])
}
fn read_matrix(
  bytes: &[u8],
  offset: usize,
) -> io::Result<[f32; 16]> {
  Ok([
    read_f32(bytes, offset)?,
    read_f32(bytes, offset + 4)?,
    read_f32(bytes, offset + 8)?,
    0.0,
    read_f32(bytes, offset + 16)?,
    read_f32(bytes, offset + 20)?,
    read_f32(bytes, offset + 24)?,
    0.0,
    read_f32(bytes, offset + 32)?,
    read_f32(bytes, offset + 36)?,
    read_f32(bytes, offset + 40)?,
    0.0,
    read_f32(bytes, offset + 48)?,
    read_f32(bytes, offset + 52)?,
    read_f32(bytes, offset + 56)?,
    1.0,
  ])
}
fn write_matrix_record(
  bytes: &mut Vec<u8>,
  matrix: &[f32; 16],
) {
  let flags = [0u32, 1, 1, 0];
  for row in 0..4 {
    for column in 0..3 {
      bytes.extend_from_slice(&matrix[row * 4 + column].to_le_bytes());
    }
    bytes.extend_from_slice(&flags[row].to_le_bytes());
  }
}
fn array<const N: usize>(
  bytes: &[u8],
  offset: usize,
) -> io::Result<[u8; N]> {
  let end = offset.checked_add(N).ok_or_else(|| invalid("YBN read offset overflows"))?;
  bytes
    .get(offset..end)
    .ok_or_else(|| invalid("YBN record is truncated"))?
    .try_into()
    .map_err(|_| invalid("YBN record is truncated"))
}
fn put_u16(
  bytes: &mut [u8],
  offset: usize,
  value: u16,
) -> io::Result<()> {
  write(bytes, offset, &value.to_le_bytes())
}
fn put_u32(
  bytes: &mut [u8],
  offset: usize,
  value: u32,
) -> io::Result<()> {
  write(bytes, offset, &value.to_le_bytes())
}
fn put_u64(
  bytes: &mut [u8],
  offset: usize,
  value: u64,
) -> io::Result<()> {
  write(bytes, offset, &value.to_le_bytes())
}
fn write_f32(
  bytes: &mut [u8],
  offset: usize,
  value: f32,
) -> io::Result<()> {
  write(bytes, offset, &value.to_le_bytes())
}
fn write_vec3(
  bytes: &mut [u8],
  offset: usize,
  value: [f32; 3],
) -> io::Result<()> {
  for (index, component) in value.iter().enumerate() {
    write_f32(bytes, offset + index * 4, *component)?;
  }
  Ok(())
}
fn write(
  bytes: &mut [u8],
  offset: usize,
  value: &[u8],
) -> io::Result<()> {
  let end = offset.checked_add(value.len()).ok_or_else(|| invalid("YBN write offset overflows"))?;
  bytes
    .get_mut(offset..end)
    .ok_or_else(|| invalid("YBN write range is out of bounds"))?
    .copy_from_slice(value);
  Ok(())
}
fn ensure_len(
  bytes: &mut Vec<u8>,
  length: usize,
) {
  if bytes.len() < length {
    bytes.resize(length, 0);
  }
}
fn reserve(
  bytes: &mut Vec<u8>,
  length: usize,
) -> io::Result<usize> {
  let padding = (16 - bytes.len() % 16) % 16;
  bytes.resize(bytes.len() + padding, 0);
  let offset = bytes.len();
  let end = offset.checked_add(length).ok_or_else(|| invalid("YBN output size overflows"))?;
  bytes.resize(end, 0);
  Ok(offset)
}
fn append(
  bytes: &mut Vec<u8>,
  value: &[u8],
) -> io::Result<usize> {
  let offset = reserve(bytes, value.len())?;
  bytes[offset..offset + value.len()].copy_from_slice(value);
  Ok(offset)
}
fn address(offset: usize) -> io::Result<u64> {
  let offset = u32::try_from(offset).map_err(|_| invalid("YBN resource exceeds address space"))?;
  Ok(BASE + offset as u64)
}
fn pointer_if_nonempty(
  offset: usize,
  length: usize,
) -> io::Result<u64> {
  if length == 0 { Ok(0) } else { address(offset) }
}
fn page_count(flags: u32) -> usize {
  ((flags >> 27) & 1) as usize
    + (((flags >> 26) & 1) << 1) as usize
    + (((flags >> 25) & 1) << 2) as usize
    + (((flags >> 24) & 1) << 3) as usize
    + (((flags >> 17) & 0x7f) << 4) as usize
    + (((flags >> 11) & 0x3f) << 5) as usize
    + (((flags >> 7) & 0xf) << 6) as usize
    + (((flags >> 5) & 3) << 7) as usize
    + (((flags >> 4) & 1) << 8) as usize
}
fn child<'a>(
  node: &'a XmlElement,
  name: &str,
) -> io::Result<&'a XmlElement> {
  node
    .children
    .iter()
    .find(|child| child.name == name)
    .ok_or_else(|| invalid(&format!("YBN XML missing {name}")))
}
fn text(node: &XmlElement) -> String {
  node.attributes.get("value").cloned().unwrap_or_else(|| node.text.trim().to_string())
}
fn number<T: std::str::FromStr>(
  node: &XmlElement,
  name: &str,
) -> io::Result<T> {
  text(child(node, name)?).parse().map_err(|_| invalid(&format!("YBN XML {name} is invalid")))
}
fn vec3(node: &XmlElement) -> io::Result<[f32; 3]> {
  Ok([float_attr(node, "x")?, float_attr(node, "y")?, float_attr(node, "z")?])
}
fn float_attr(
  node: &XmlElement,
  name: &str,
) -> io::Result<f32> {
  node
    .attributes
    .get(name)
    .ok_or_else(|| invalid(&format!("YBN vector missing {name}")))?
    .parse()
    .map_err(|_| invalid("YBN vector coordinate is invalid"))
}
fn read_float_array(
  node: &XmlElement,
  name: &str,
  count: usize,
) -> io::Result<[f32; 16]> {
  let values = numbers(&child(node, name)?.text)?;
  if values.len() != count {
    return Err(invalid(&format!("YBN {name} requires {count} values")));
  }
  values.try_into().map_err(|_| invalid("YBN matrix has the wrong number of values"))
}
fn numbers(value: &str) -> io::Result<Vec<f32>> {
  value
    .split(|c: char| c.is_whitespace() || c == ',')
    .filter(|token| !token.is_empty())
    .map(|token| token.parse().map_err(|_| invalid("YBN numeric array contains an invalid value")))
    .collect()
}
fn parse_vectors(value: &str) -> io::Result<Vec<[f32; 3]>> {
  value
    .lines()
    .filter(|line| !line.trim().is_empty())
    .map(|line| {
      let values = numbers(line)?;
      if values.len() != 3 {
        return Err(invalid("YBN vertex must have three coordinates"));
      }
      Ok([values[0], values[1], values[2]])
    })
    .collect()
}
fn parse_colors(value: &str) -> io::Result<Vec<[u8; 4]>> {
  value
    .lines()
    .filter(|line| !line.trim().is_empty())
    .map(|line| {
      let values = line
        .split(',')
        .map(str::trim)
        .map(|item| item.parse::<u8>().map_err(|_| invalid("YBN color channel is invalid")))
        .collect::<io::Result<Vec<_>>>()?;
      if values.len() != 4 {
        return Err(invalid("YBN color must have four channels"));
      }
      Ok([values[0], values[1], values[2], values[3]])
    })
    .collect()
}
fn polygon_attr<T: std::str::FromStr>(
  node: &XmlElement,
  name: &str,
) -> io::Result<T> {
  node
    .attributes
    .get(name)
    .ok_or_else(|| invalid(&format!("YBN polygon missing {name}")))?
    .parse()
    .map_err(|_| invalid("YBN polygon attribute is invalid"))
}
fn parse_material_flags(text: &str) -> io::Result<u16> {
  parse_named_flags(
    text,
    &[
      "FLAG_STAIRS",
      "FLAG_NOT_CLIMBABLE",
      "FLAG_SEE_THROUGH",
      "FLAG_SHOOT_THROUGH",
      "FLAG_NOT_COVER",
      "FLAG_WALKABLE_PATH",
      "FLAG_NO_CAM_COLLISION",
      "FLAG_SHOOT_THROUGH_FX",
      "FLAG_NO_DECAL",
      "FLAG_NO_NAVMESH",
      "FLAG_NO_RAGDOLL",
      "FLAG_VEHICLE_WHEEL",
      "FLAG_NO_PTFX",
      "FLAG_TOO_STEEP_FOR_PLAYER",
      "FLAG_NO_NETWORK_SPAWN",
      "FLAG_NO_CAM_COLLISION_ALLOW_CLIPPING",
    ],
  )
}
fn parse_composite_flags(text: &str) -> io::Result<u32> {
  parse_named_flags(
    text,
    &[
      "UNKNOWN",
      "MAP_WEAPON",
      "MAP_DYNAMIC",
      "MAP_ANIMAL",
      "MAP_COVER",
      "MAP_VEHICLE",
      "VEHICLE_NOT_BVH",
      "VEHICLE_BVH",
      "VEHICLE_BOX",
      "PED",
      "RAGDOLL",
      "ANIMAL",
      "ANIMAL_RAGDOLL",
      "OBJECT",
      "OBJECT_ENV_CLOTH",
      "PLANT",
      "PROJECTILE",
      "EXPLOSION",
      "PICKUP",
      "FOLIAGE",
      "FORKLIFT_FORKS",
      "TEST_WEAPON",
      "TEST_CAMERA",
      "TEST_AI",
      "TEST_SCRIPT",
      "TEST_VEHICLE_WHEEL",
      "GLASS",
      "MAP_RIVER",
      "SMOKE",
      "UNSMASHED",
      "MAP_STAIRS",
      "MAP_DEEP_SURFACE",
    ],
  )
}
fn parse_named_flags<T: TryFrom<u64>>(
  text: &str,
  names: &[&str],
) -> io::Result<T> {
  let mut bits = 0u64;
  for name in text.split(',').map(str::trim).filter(|name| !name.is_empty() && *name != "NONE") {
    let index = names
      .iter()
      .position(|candidate| *candidate == name)
      .ok_or_else(|| invalid(&format!("unknown YBN flag {name}")))?;
    bits |= 1 << index;
  }
  T::try_from(bits).map_err(|_| invalid("YBN flags exceed supported width"))
}
fn material_flag_text(flags: u16) -> String {
  named_flag_text(
    flags as u64,
    &[
      "FLAG_STAIRS",
      "FLAG_NOT_CLIMBABLE",
      "FLAG_SEE_THROUGH",
      "FLAG_SHOOT_THROUGH",
      "FLAG_NOT_COVER",
      "FLAG_WALKABLE_PATH",
      "FLAG_NO_CAM_COLLISION",
      "FLAG_SHOOT_THROUGH_FX",
      "FLAG_NO_DECAL",
      "FLAG_NO_NAVMESH",
      "FLAG_NO_RAGDOLL",
      "FLAG_VEHICLE_WHEEL",
      "FLAG_NO_PTFX",
      "FLAG_TOO_STEEP_FOR_PLAYER",
      "FLAG_NO_NETWORK_SPAWN",
      "FLAG_NO_CAM_COLLISION_ALLOW_CLIPPING",
    ],
  )
}
fn composite_flag_text(flags: u32) -> String {
  named_flag_text(
    flags as u64,
    &[
      "UNKNOWN",
      "MAP_WEAPON",
      "MAP_DYNAMIC",
      "MAP_ANIMAL",
      "MAP_COVER",
      "MAP_VEHICLE",
      "VEHICLE_NOT_BVH",
      "VEHICLE_BVH",
      "VEHICLE_BOX",
      "PED",
      "RAGDOLL",
      "ANIMAL",
      "ANIMAL_RAGDOLL",
      "OBJECT",
      "OBJECT_ENV_CLOTH",
      "PLANT",
      "PROJECTILE",
      "EXPLOSION",
      "PICKUP",
      "FOLIAGE",
      "FORKLIFT_FORKS",
      "TEST_WEAPON",
      "TEST_CAMERA",
      "TEST_AI",
      "TEST_SCRIPT",
      "TEST_VEHICLE_WHEEL",
      "GLASS",
      "MAP_RIVER",
      "SMOKE",
      "UNSMASHED",
      "MAP_STAIRS",
      "MAP_DEEP_SURFACE",
    ],
  )
}
fn named_flag_text(
  flags: u64,
  names: &[&str],
) -> String {
  let selected = names
    .iter()
    .enumerate()
    .filter(|(bit, _)| flags & (1 << bit) != 0)
    .map(|(_, name)| *name)
    .collect::<Vec<_>>();
  if selected.is_empty() { "NONE".into() } else { selected.join(", ") }
}
fn indent(
  xml: &mut String,
  depth: usize,
) {
  xml.extend(std::iter::repeat_n(' ', depth));
}
fn val_tag(
  xml: &mut String,
  depth: usize,
  name: &str,
  value: impl std::fmt::Display,
) {
  indent(xml, depth);
  xml.push_str(&format!("<{name} value=\"{value}\" />\n"));
}
fn vec_tag(
  xml: &mut String,
  depth: usize,
  name: &str,
  value: [f32; 3],
) {
  indent(xml, depth);
  xml.push_str(&format!("<{name} x=\"{}\" y=\"{}\" z=\"{}\" />\n", value[0], value[1], value[2]));
}
fn text_tag(
  xml: &mut String,
  depth: usize,
  name: &str,
  value: &str,
) {
  indent(xml, depth);
  xml.push_str(&format!("<{name}>"));
  write_text_content(xml, depth, value);
  xml.push_str(&format!("</{name}>\n"));
}
fn write_array_open(
  xml: &mut String,
  depth: usize,
  name: &str,
) {
  indent(xml, depth);
  xml.push_str(&format!("<{name}>\n"));
}
fn write_array_close(
  xml: &mut String,
  depth: usize,
  name: &str,
) {
  indent(xml, depth);
  xml.push_str(&format!("</{name}>\n"));
}
fn format_vectors(values: &[[f32; 3]]) -> String {
  values.iter().map(|v| format!("{}, {}, {}", v[0], v[1], v[2])).collect::<Vec<_>>().join("\n")
}
fn format_colors(values: &[[u8; 4]]) -> String {
  values
    .iter()
    .map(|c| format!("{}, {}, {}, {}", c[0], c[1], c[2], c[3]))
    .collect::<Vec<_>>()
    .join("\n")
}
fn identity() -> [f32; 16] {
  [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]
}
fn invalid(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
  use super::{
    BOUNDS_SIZE, Bound, COMPOSITE_SIZE, GEOMETRY_BVH_SIZE, Geometry, Material, Polygon,
    PolygonLookup, Triangle, child, encode_ybn_bound, geometry_polygon_records, identity,
    merge_ybn_deltas, parse_xml, read_ybn_root, vec3, write_f32, write_vec3, xml_to_ybn,
    ybn_to_xml,
  };
  use std::path::Path;

  fn sample(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/ybn_conflicts").join(name);
    let xml = std::fs::read_to_string(path).unwrap();
    xml_to_ybn(&xml).unwrap()
  }

  fn merged_children(bytes: &[u8]) -> (usize, [f32; 3], [f32; 3]) {
    let xml = ybn_to_xml(bytes).unwrap();
    let root = parse_xml(&xml).unwrap();
    let bounds = child(&root, "Bounds").unwrap();
    let children = child(bounds, "Children").unwrap();
    (
      children.children.len(),
      vec3(child(bounds, "BoxMin").unwrap()).unwrap(),
      vec3(child(bounds, "BoxMax").unwrap()).unwrap(),
    )
  }

  fn triangle_root(
    vertices: &[[f32; 3]],
    triangles: &[([u16; 3], [bool; 3])],
  ) -> Vec<u8> {
    let minimum = std::array::from_fn(|axis| {
      vertices.iter().map(|vertex| vertex[axis]).fold(f32::INFINITY, f32::min)
    });
    let maximum = std::array::from_fn(|axis| {
      vertices.iter().map(|vertex| vertex[axis]).fold(f32::NEG_INFINITY, f32::max)
    });
    let center = std::array::from_fn(|axis| (minimum[axis] + maximum[axis]) * 0.5);
    let radius = (0..3).map(|axis| (maximum[axis] - center[axis]).powi(2)).sum::<f32>().sqrt();
    let mut common = vec![0; BOUNDS_SIZE];
    common[16] = 10;
    write_f32(&mut common, 20, radius).unwrap();
    write_vec3(&mut common, 32, maximum).unwrap();
    write_vec3(&mut common, 48, minimum).unwrap();
    write_vec3(&mut common, 64, center).unwrap();
    write_vec3(&mut common, 80, center).unwrap();

    let mut geometry_common = common.clone();
    geometry_common[16] = 8;
    let geometry = Geometry {
      vertices: vertices.to_vec(),
      materials: vec![Material {
        kind: 1,
        ..Material::default()
      }],
      polygons: triangles
        .iter()
        .map(|(vertices, vertex_flags)| {
          Polygon::Triangle(Triangle {
            material: 0,
            vertices: *vertices,
            vertex_flags: *vertex_flags,
            ..Triangle::default()
          })
        })
        .collect(),
      ..Geometry::default()
    };
    let geometry_bound = Bound {
      kind: "GeometryBVH".to_string(),
      common: geometry_common,
      extension: vec![0; GEOMETRY_BVH_SIZE - BOUNDS_SIZE],
      geometry: Some(geometry),
      children: Vec::new(),
      transform: Some(identity()),
      composite_flags: [0; 2],
      transforms: Vec::new(),
      flags: Vec::new(),
    };
    let root = Bound {
      kind: "Composite".to_string(),
      common,
      extension: vec![0; COMPOSITE_SIZE - BOUNDS_SIZE],
      geometry: None,
      children: vec![geometry_bound],
      transform: None,
      composite_flags: [0; 2],
      transforms: vec![identity()],
      flags: vec![[0; 2]],
    };
    encode_ybn_bound(&root).unwrap()
  }

  #[test]
  fn multiline_geometry_text_arrays_are_indented_and_rebuildable() {
    let original = triangle_root(
      &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
      &[([0, 1, 2], [false; 3])],
    );
    let mut root = read_ybn_root(&original).unwrap();
    let geometry = root.children[0].geometry.as_mut().unwrap();
    geometry.material_colours = vec![[1, 2, 3, 4], [5, 6, 7, 8]];
    geometry.vertex_colours = vec![[10, 20, 30, 40]; 3];
    let bytes = encode_ybn_bound(&root).unwrap();
    let xml = ybn_to_xml(&bytes).unwrap();
    for name in ["Vertices", "MaterialColours", "VertexColours"] {
      assert!(xml.contains(&format!("    <{name}>\n     ")), "missing formatted {name}");
      assert!(xml.contains(&format!("\n    </{name}>")), "misaligned closing {name}");
    }
    assert!(xml.contains("<Flags>NONE</Flags>"));
    let rebuilt = xml_to_ybn(&xml).unwrap();
    let actual = read_ybn_root(&rebuilt).unwrap();
    let before = read_ybn_root(&bytes).unwrap();
    let expected_geometry = before.children[0].geometry.as_ref().unwrap();
    let actual_geometry = actual.children[0].geometry.as_ref().unwrap();
    assert_eq!(actual_geometry.material_colours, expected_geometry.material_colours);
    assert_eq!(actual_geometry.vertex_colours, expected_geometry.vertex_colours);
    let expected = geometry_polygon_records(&before).unwrap();
    let actual = geometry_polygon_records(&actual).unwrap();
    assert_eq!(actual.len(), expected.len());
    assert!(
      actual
        .iter()
        .zip(&expected)
        .all(|(actual, expected)| actual.identity.matches(&expected.identity))
    );
  }

  #[test]
  fn non_manifold_shared_edges_are_stable_after_bvh_ordering() {
    let source = triangle_root(
      &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]],
      &[([0, 1, 2], [false; 3]), ([1, 0, 3], [false; 3]), ([0, 1, 4], [false; 3])],
    );
    let mut original = read_ybn_root(&source).unwrap();
    original.children[0].geometry.as_mut().unwrap().polygons.reverse();
    let mut xml = "<BoundsFile>\n".to_string();
    super::write_bound_xml(&original, 1, "Bounds", &mut xml);
    xml.push_str("</BoundsFile>\n");
    let first = xml_to_ybn(&xml).unwrap();
    let first_xml = ybn_to_xml(&first).unwrap();
    let second = xml_to_ybn(&first_xml).unwrap();
    assert_eq!(first, second);
    assert_eq!(first_xml, ybn_to_xml(&second).unwrap());
    let rebuilt = read_ybn_root(&first).unwrap();
    let geometry = rebuilt.children[0].geometry.as_ref().unwrap();
    let shared = geometry
      .polygons
      .iter()
      .enumerate()
      .filter_map(|(index, polygon)| {
        if let Polygon::Triangle(triangle) = polygon
          && matches!([triangle.vertices[0], triangle.vertices[1]], [0, 1] | [1, 0])
        {
          return Some((index as u16, triangle.edge_indices[0]));
        }
        None
      })
      .collect::<Vec<_>>();
    assert_eq!(shared.len(), 3);
    assert_eq!(shared[0].1, shared[1].0);
    assert_eq!(shared[1].1, shared[0].0);
    assert_eq!(shared[2].1, shared[0].0);
  }

  #[test]
  fn rebuilding_geometry_bvh_preserves_the_source_vertex_quantum() {
    let source = triangle_root(
      &[[-102.6387, 0.00031, 15.2345], [-101.1173, 0.00093, 15.2381], [-102.2251, 1.0177, 15.2319]],
      &[([0, 1, 2], [false; 3])],
    );
    let original = read_ybn_root(&source).unwrap();
    let expected = geometry_polygon_records(&original).unwrap();

    let rebuilt = encode_ybn_bound(&original).unwrap();
    let rebuilt_root = read_ybn_root(&rebuilt).unwrap();
    let actual = geometry_polygon_records(&rebuilt_root).unwrap();

    assert_eq!(actual.len(), expected.len());
    assert!(
      actual
        .iter()
        .zip(&expected)
        .all(|(actual, expected)| actual.identity.matches(&expected.identity))
    );
  }

  #[test]
  fn merges_geometry_triangle_additions_and_removals_by_coordinates() {
    let vanilla = triangle_root(
      &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.0]],
      &[([0, 1, 2], [false; 3]), ([1, 3, 2], [false; 3])],
    );
    let modded = triangle_root(
      &[
        [0.0, 1.0, 0.0],
        [1.0, 1.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [2.0, 0.0, 0.0],
        [2.0, 1.0, 0.0],
      ],
      &[([2, 1, 0], [false; 3]), ([2, 4, 5], [false; 3])],
    );

    let merged = merge_ybn_deltas(&vanilla, &[&modded]).unwrap();
    let root = read_ybn_root(&merged).unwrap();
    let actual = geometry_polygon_records(&root).unwrap();
    let modded_root = read_ybn_root(&modded).unwrap();
    let expected = geometry_polygon_records(&modded_root).unwrap();

    assert_eq!(actual.len(), 2, "one vanilla triangle is removed, one survives, and one is added");
    let mut expected_lookup = super::PolygonLookup::default();
    for record in expected {
      expected_lookup.insert(record.identity);
    }
    assert!(actual.iter().all(|record| expected_lookup.contains(&record.identity)));
    assert_eq!(root.children.len(), 2, "only the added triangle should need a new geometry child");
  }

  #[test]
  fn merges_mixed_geometry_polygons_with_quantization_tolerance() {
    let triangle_vertices = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let triangle = triangle_root(&triangle_vertices, &[([0, 1, 2], [false; 3])]);
    let mut vanilla_root = read_ybn_root(&triangle).unwrap();
    let geometry = vanilla_root.children[0].geometry.as_mut().unwrap();
    geometry.vertices.extend([[0.1, 0.1, 0.0], [0.2, 0.1, 0.0], [0.2, 0.2, 0.0], [0.1, 0.2, 0.0]]);
    geometry.polygons.push(Polygon::Box {
      material: 0,
      vertices: [3, 4, 5, 6],
    });
    let vanilla = encode_ybn_bound(&vanilla_root).unwrap();

    let mut mod_root = read_ybn_root(&vanilla).unwrap();
    let geometry = mod_root.children[0].geometry.as_mut().unwrap();
    for vertex in &mut geometry.vertices {
      for coordinate in vertex {
        *coordinate += 0.002;
      }
    }
    geometry.polygons.retain(|polygon| !matches!(polygon, Polygon::Box { .. }));
    geometry.vertices.extend([[0.7, 0.7, 0.0], [0.8, 0.7, 0.0], [0.8, 0.8, 0.0], [0.7, 0.8, 0.0]]);
    geometry.polygons.push(Polygon::Box {
      material: 0,
      vertices: [7, 8, 9, 10],
    });
    let modded = encode_ybn_bound(&mod_root).unwrap();

    let merged = merge_ybn_deltas(&vanilla, &[&modded]).unwrap();
    let merged_root = read_ybn_root(&merged).unwrap();
    let result_records = geometry_polygon_records(&merged_root).unwrap();
    let expected_records = geometry_polygon_records(&read_ybn_root(&modded).unwrap()).unwrap();
    let mut expected = PolygonLookup::default();
    for record in expected_records {
      expected.insert(record.identity);
    }

    assert_eq!(merged_root.children.len(), 2);
    assert_eq!(result_records.len(), 2, "one triangle and one replacement box must remain");
    assert!(result_records.iter().all(|record| expected.contains(&record.identity)));
  }

  #[test]
  fn merges_mod_child_additions_and_removals_against_vanilla() {
    let vanilla = sample("resource_a.ybn.xml");
    let unchanged_mod = sample("resource_a.ybn.xml");
    let changed_mod = sample("resource_b.ybn.xml");
    let merged = merge_ybn_deltas(&vanilla, &[&unchanged_mod, &changed_mod]).unwrap();
    let (children, minimum, maximum) = merged_children(&merged);

    assert_eq!(children, 1, "the removed vanilla child must not survive");
    assert_eq!(minimum[0], -1.0, "the root broadphase must retain the baseline extent");
    assert_eq!(maximum[0], 11.0);
  }

  #[test]
  fn merges_independent_additions_from_multiple_mods() {
    let vanilla = sample("vanilla_empty.ybn.xml");
    let first_mod = sample("resource_a.ybn.xml");
    let second_mod = sample("resource_b.ybn.xml");
    let merged = merge_ybn_deltas(&vanilla, &[&first_mod, &second_mod, &first_mod]).unwrap();
    let (children, minimum, maximum) = merged_children(&merged);

    assert_eq!(children, 2, "the same addition from two mods must be deduplicated");
    assert_eq!(minimum[0], -1.0);
    assert_eq!(maximum[0], 11.0);
  }
}
