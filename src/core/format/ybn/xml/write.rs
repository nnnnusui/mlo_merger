//! Writes XML Bounds and multiline geometry arrays.

use super::super::*;

pub(in crate::core::format::ybn) fn write_bound_xml(
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
