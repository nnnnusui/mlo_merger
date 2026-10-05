//! Native YBN conversion and collision-merge regression tests.

use super::{
  BOUNDS_SIZE, Bound, COMPOSITE_SIZE, GEOMETRY_BVH_SIZE, Geometry, Material, Polygon,
  PolygonLookup, Triangle, child, encode_ybn_bound, geometry_polygon_records, identity,
  merge_ybn_deltas, parse_xml, read_ybn_root, vec3, write_f32, write_vec3, xml_to_ybn, ybn_to_xml,
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
  let original =
    triangle_root(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], &[([0, 1, 2], [false; 3])]);
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
