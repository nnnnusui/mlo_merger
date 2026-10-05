//! Reads XML Bounds and indexed collision geometry.

use super::super::*;

pub(in crate::core::format::ybn) fn read_bound_xml(
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

/// The optional Native quantum preserves the vertex grid across rebuilds;
/// standard CodeWalker XML without it still uses the calculated grid.
pub(in crate::core::format::ybn) fn geometry_from_xml(node: &XmlElement) -> io::Result<Geometry> {
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
