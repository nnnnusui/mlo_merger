use super::*;
use crate::core::format::ybn::{
  model::{Bound, Geometry, Material, Polygon, Triangle},
  read_ybn,
  xml::{read_xml, xml_to_ybn},
};

fn geometry(polygons: Vec<Polygon>) -> Bound {
  let mut common = vec![0; 112];
  common[16] = 8;
  Bound {
    kind: "GeometryBVH".into(),
    common,
    extension: vec![],
    geometry: Some(Geometry {
      vertices: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
      materials: vec![Material::default()],
      polygons,
      ..Geometry::default()
    }),
    children: vec![],
    transform: None,
    composite_flags: [0; 2],
    transforms: vec![],
    flags: vec![],
  }
}

fn triangle() -> Polygon {
  Polygon::Triangle(Triangle {
    vertices: [0, 1, 2],
    vertex_flags: [false, true, false],
    ..Triangle::default()
  })
}

fn composite(children: Vec<Bound>) -> Bound {
  let mut common = vec![0; 112];
  common[16] = 10;
  Bound {
    kind: "Composite".into(),
    common,
    extension: vec![],
    geometry: None,
    flags: vec![[0; 2]; children.len()],
    transforms: vec![[0.0; 16]; children.len()],
    children,
    transform: None,
    composite_flags: [0; 2],
  }
}

#[test]
fn semantic_diff_ignores_indices_cyclic_winding_and_derived_metadata() {
  let mut before = geometry(vec![triangle()]);
  let source = before.geometry.as_mut().unwrap();
  source.vertex_colours = vec![[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12], [13, 14, 15, 16]];
  source.materials.push(Material {
    kind: 3,
    ..Material::default()
  });
  source.material_colours = vec![[20, 30, 40, 50], [60, 70, 80, 90]];
  let mut after = before.clone();
  let target = after.geometry.as_mut().unwrap();
  target.vertices.reverse();
  target.vertex_colours.reverse();
  target.materials.reverse();
  target.material_colours.reverse();
  target.materials[1].colour_index = 1;
  target.vertex_quantum = Some([0.25; 3]);
  target.polygons[0] = Polygon::Triangle(Triangle {
    material: 1,
    vertices: [2, 1, 3],
    vertex_flags: [true, false, false],
    area: 999.0,
    edge_indices: [3, 4, 5],
  });
  assert!(YbnDiff::extract_from(&before, &after).unwrap().is_empty());
}

#[test]
fn semantic_diff_preserves_duplicate_face_multiplicity_and_json_roundtrip() {
  let before = geometry(vec![triangle(), triangle(), triangle()]);
  let after = geometry(vec![triangle()]);
  let removed = YbnDiff::extract_from(&before, &after).unwrap();
  assert_eq!(removed.polygon_diffs.len(), 2);
  assert!(removed.polygon_diffs.iter().all(|diff| matches!(diff, PolygonDiff::Removed { .. })));
  let added = YbnDiff::extract_from(&after, &before).unwrap();
  assert_eq!(added.polygon_diffs.len(), 2);
  assert!(added.polygon_diffs.iter().all(|diff| matches!(diff, PolygonDiff::Added { .. })));
  let json = serde_json::to_string(&removed).unwrap();
  assert_eq!(serde_json::from_str::<YbnDiff>(&json).unwrap(), removed);
}

#[test]
fn semantic_diff_handles_all_primitive_types_and_radius_changes() {
  let before = geometry(vec![
    triangle(),
    Polygon::Box {
      material: 0,
      vertices: [0, 1, 2, 3],
    },
    Polygon::Sphere {
      material: 0,
      vertex: 0,
      radius: 1.0,
    },
    Polygon::Capsule {
      material: 0,
      vertex1: 0,
      vertex2: 1,
      radius: 2.0,
    },
    Polygon::Cylinder {
      material: 0,
      vertex1: 1,
      vertex2: 2,
      radius: 3.0,
    },
  ]);
  let mut after = before.clone();
  let target = after.geometry.as_mut().unwrap();
  target.polygons.remove(1);
  target.polygons[1] = Polygon::Sphere {
    material: 0,
    vertex: 0,
    radius: 1.5,
  };
  target.polygons[2] = Polygon::Capsule {
    material: 0,
    vertex1: 1,
    vertex2: 0,
    radius: 2.0,
  };
  let diff = YbnDiff::extract_from(&before, &after).unwrap();
  assert_eq!(diff.polygon_diffs.len(), 3);
  assert_eq!(
    diff.polygon_diffs.iter().filter(|diff| matches!(diff, PolygonDiff::Removed { .. })).count(),
    2
  );
  assert_eq!(
    diff.polygon_diffs.iter().filter(|diff| matches!(diff, PolygonDiff::Added { .. })).count(),
    1
  );
}

#[test]
fn semantic_diff_keeps_ownership_and_reports_transform_changes_separately() {
  let before = composite(vec![composite(vec![geometry(vec![triangle()])])]);
  let mut after = before.clone();
  after.children[0].transform = Some([1.0; 16]);
  let diff = YbnDiff::extract_from(&before, &after).unwrap();
  assert!(diff.polygon_diffs.is_empty());
  assert_eq!(diff.bound_diffs.len(), 1);
  assert!(matches!(&diff.bound_diffs[0], BoundDiff::Modified { path, .. } if path == &[0]));
  let shifted = composite(vec![composite(vec![geometry(vec![]), geometry(vec![triangle()])])]);
  let diff = YbnDiff::extract_from(&before, &shifted).unwrap();
  assert_eq!(diff.polygon_diffs.len(), 2);
  assert!(
    matches!(&diff.polygon_diffs[0], PolygonDiff::Removed { location, .. } if location.bound_path == [0, 0])
  );
  assert!(
    matches!(&diff.polygon_diffs[1], PolygonDiff::Added { location, .. } if location.bound_path == [0, 1])
  );
}

#[test]
fn semantic_diff_detects_reversed_winding_and_uses_coordinate_tolerance() {
  let before = geometry(vec![triangle()]);
  let mut after = before.clone();
  for position in &mut after.geometry.as_mut().unwrap().vertices {
    position[0] += 0.002;
  }
  assert!(YbnDiff::extract_from(&before, &after).unwrap().is_empty());
  after.geometry.as_mut().unwrap().polygons[0] = Polygon::Triangle(Triangle {
    vertices: [0, 2, 1],
    vertex_flags: [false, false, true],
    ..Triangle::default()
  });
  assert_eq!(YbnDiff::extract_from(&before, &after).unwrap().polygon_diffs.len(), 2);
  assert!(YbnDiff::with_tolerance(&before, &after, 0.0).is_err());
  assert!(YbnDiff::with_tolerance(&before, &after, f32::NAN).is_err());
}

#[test]
fn semantic_diff_rejects_unmodeled_records_and_invalid_references() {
  let before = geometry(vec![triangle()]);
  let mut after = before.clone();
  after.geometry.as_mut().unwrap().polygons[0] = Polygon::Unsupported {
    raw: [0; 16],
  };
  assert!(YbnDiff::extract_from(&before, &after).unwrap_err().to_string().contains("Opaque"));
  after.geometry.as_mut().unwrap().polygons[0] = Polygon::Sphere {
    material: 0,
    vertex: 55,
    radius: 1.0,
  };
  assert!(YbnDiff::extract_from(&before, &after).is_err());
}

#[test]
fn semantic_diff_uses_shared_xml_and_native_models() {
  let source = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/sample/ybn_conflicts/geometry_bvh.ybn.xml"
  ));
  let binary = xml_to_ybn(source).unwrap();
  let native = read_ybn(&binary).unwrap();
  let exported = crate::core::format::ybn::ybn_to_xml(&binary).unwrap();
  let parsed = read_xml(&exported).unwrap();
  assert!(YbnDiff::extract_from(&native, &parsed).unwrap().is_empty());
  let mut modified = parsed.clone();
  let owner = if modified.geometry.is_some() { &mut modified } else { &mut modified.children[0] };
  owner.geometry.as_mut().unwrap().polygons.pop().unwrap();
  assert_eq!(YbnDiff::extract_from(&parsed, &modified).unwrap().polygon_diffs.len(), 1);
}

#[test]
fn semantic_diff_real_brofx_model_preserves_exact_polygon_occurrence_counts() {
  let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("asset/source/[patron]/brofx_mansion_06/stream/ch2_06_1.ybn");
  let before = read_ybn(&std::fs::read(path).unwrap()).unwrap();
  assert!(YbnDiff::extract_from(&before, &before).unwrap().is_empty());
  let mut after = before.clone();
  let owner = after
    .children
    .iter_mut()
    .find(|bound| bound.geometry.as_ref().is_some_and(|geometry| !geometry.polygons.is_empty()))
    .unwrap();
  owner.geometry.as_mut().unwrap().polygons.pop().unwrap();
  let diff = YbnDiff::extract_from(&before, &after).unwrap();
  assert!(diff.bound_diffs.is_empty());
  assert_eq!(diff.polygon_diffs.len(), 1);
  assert!(matches!(&diff.polygon_diffs[0], PolygonDiff::Removed { .. }));
  let json = serde_json::to_vec(&diff).unwrap();
  assert_eq!(serde_json::from_slice::<YbnDiff>(&json).unwrap(), diff);
}

#[test]
fn semantic_diff_prefers_exact_occurrences_over_nearby_candidates() {
  let mut before = geometry(vec![
    Polygon::Sphere {
      material: 0,
      vertex: 0,
      radius: 1.0,
    },
    Polygon::Sphere {
      material: 0,
      vertex: 1,
      radius: 1.0,
    },
    Polygon::Sphere {
      material: 0,
      vertex: 2,
      radius: 1.0,
    },
  ]);
  before.geometry.as_mut().unwrap().vertices =
    vec![[0.0; 3], [0.004, 0.0, 0.0], [-0.004, 0.0, 0.0]];
  assert!(YbnDiff::extract_from(&before, &before).unwrap().is_empty());
}
