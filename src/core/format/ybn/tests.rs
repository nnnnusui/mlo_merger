//! Native YBN conversion and collision-merge regression tests.

use super::{
  BOUNDS_SIZE, Bound, COMPOSITE_SIZE, GEOMETRY_BVH_SIZE, Geometry, Material, Polygon, Triangle,
  child, diff::polygons, encode_ybn_bound, identity, merge_ybn_deltas, parse_xml, read_ybn_root,
  vec3, write_f32, write_vec3, xml_to_ybn, ybn_to_xml,
};
use std::path::Path;

fn sample(name: &str) -> Vec<u8> {
  let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/sample/ybn_conflicts").join(name);
  let xml = crate::platform::fs::read_to_string(path).unwrap();
  xml_to_ybn(&xml).unwrap()
}

/// The polygons `first` has and `second` lacks, and those `second` has and `first` lacks, by
/// flags, within the default tolerance.
fn polygon_differences(
  first: &Bound,
  second: &Bound,
) -> (usize, usize) {
  let diff = polygons::diff(first, second, 0.05).unwrap();
  (
    diff.polygons.iter().map(|group| group.removed.len()).sum(),
    diff.polygons.iter().map(|group| group.added.len()).sum(),
  )
}

/// The number of geometry polygons of `root`.
fn polygon_count(root: &Bound) -> usize {
  root.children.iter().filter_map(|child| child.geometry.as_ref()).map(|g| g.polygons.len()).sum()
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
fn rebuild_replaces_source_only_fixup_pointers() {
  let bytes =
    triangle_root(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], &[([0, 1, 2], [false; 3])]);
  let mut root = read_ybn_root(&bytes).unwrap();
  let poison = 0xdead_beef_dead_beefu64.to_le_bytes();
  root.common[8..16].copy_from_slice(&poison);
  for offset in [128, 136, 168] {
    root.extension[offset - BOUNDS_SIZE..offset - BOUNDS_SIZE + 8].copy_from_slice(&poison);
  }
  let geometry = &mut root.children[0];
  geometry.common[8..16].copy_from_slice(&poison);
  for offset in [120, 192, 200, 304] {
    geometry.extension[offset - BOUNDS_SIZE..offset - BOUNDS_SIZE + 8].copy_from_slice(&poison);
  }
  let rebuilt = encode_ybn_bound(&root).unwrap();
  let resource = super::Rsc7Resource::decode(&rebuilt).unwrap();
  let read_pointer =
    |bytes: &[u8], offset| u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
  let record = &resource.system_data;
  assert_eq!(read_pointer(record, 128), read_pointer(record, 120));
  let boxes = resource.read_address(read_pointer(record, 136), 32).unwrap();
  assert_eq!(read_pointer(record, 168), 0);
  let table = resource.read_address(read_pointer(record, 112), 8).unwrap();
  let child = resource.read_address(read_pointer(table, 0), GEOMETRY_BVH_SIZE).unwrap();
  assert_eq!(&boxes[..12], &child[48..60]);
  assert_eq!(&boxes[12..16], &1u32.to_le_bytes());
  assert_eq!(&boxes[16..28], &child[32..44]);
  assert_eq!(&boxes[28..32], &child[44..48]);
  for offset in [8, 120, 192, 200] {
    assert_eq!(read_pointer(child, offset), 0, "stale child pointer at {offset:#x}");
  }
  resource.read_address(read_pointer(record, 8), 16).unwrap();
  resource.read_address(read_pointer(child, 304), 128).unwrap();
  assert_eq!(read_ybn_root(&rebuilt).unwrap().children.len(), 1);
}

/// The local file whose path, relative to the repository, the environment variable `variable`
/// names, for tests of cached source files that cannot be shipped.
fn local_source(variable: &str) -> Vec<u8> {
  let path = std::env::var(variable).unwrap_or_else(|_| panic!("set {variable}"));
  crate::platform::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).unwrap()
}

#[test]
#[ignore = "requires local cs6_05_2 and ch1_04b_0 vanilla files and cached source YBNs named by MLO_CS6_05_2_YBN, MLO_CS6_05_2_SECOND_YBN and MLO_CH1_04B_0_YBN"]
fn real_collision_rebuilds_have_relocated_fixup_tables() {
  let source = local_source("MLO_CS6_05_2_YBN");
  let second = local_source("MLO_CS6_05_2_SECOND_YBN");
  let vanilla = crate::platform::fs::read(
    crate::app::map::vanilla_cache::test_support::asset_latest_file("cs6_05_2.ybn"),
  )
  .unwrap();
  let merged = merge_ybn_deltas(&vanilla, &[&source, &second], Default::default()).unwrap();
  assert_relocated_fixup_tables(&merged);
  let ch1_04b_0 = local_source("MLO_CH1_04B_0_YBN");
  let rebuilt = encode_ybn_bound(&read_ybn_root(&ch1_04b_0).unwrap()).unwrap();
  assert_relocated_fixup_tables(&rebuilt);
}

fn assert_relocated_fixup_tables(bytes: &[u8]) {
  fn pointer(
    bytes: &[u8],
    offset: usize,
  ) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
  }
  fn inspect(
    resource: &super::Rsc7Resource,
    address: u64,
    root: bool,
  ) {
    let record = resource.read_address(address, BOUNDS_SIZE).unwrap();
    if root {
      resource.read_address(pointer(record, 8), 16).unwrap();
    } else {
      assert_eq!(pointer(record, 8), 0);
    }
    if record[16] == 10 {
      let record = resource.read_address(address, COMPOSITE_SIZE).unwrap();
      let count = u16::from_le_bytes(record[160..162].try_into().unwrap()) as usize;
      assert_eq!(pointer(record, 120), pointer(record, 128));
      assert_eq!(pointer(record, 168), 0);
      if count == 0 {
        return;
      }
      let children = resource.read_address(pointer(record, 112), count * 8).unwrap();
      resource.read_address(pointer(record, 120), count * 64).unwrap();
      let boxes = resource.read_address(pointer(record, 136), count * 32).unwrap();
      for index in 0..count {
        let child_address = pointer(children, index * 8);
        let bbox = &boxes[index * 32..(index + 1) * 32];
        if child_address == 0 {
          assert_eq!(bbox, [0; 32]);
          continue;
        }
        let child = resource.read_address(child_address, BOUNDS_SIZE).unwrap();
        assert_eq!(&bbox[..12], &child[48..60]);
        assert_eq!(&bbox[16..28], &child[32..44]);
        assert_eq!(&bbox[28..32], &child[44..48]);
        inspect(resource, child_address, false);
      }
    } else if matches!(record[16], 4 | 8) {
      let length = if record[16] == 8 { GEOMETRY_BVH_SIZE } else { super::GEOMETRY_SIZE };
      let record = resource.read_address(address, length).unwrap();
      for offset in [120, 192, 200] {
        assert_eq!(pointer(record, offset), 0);
      }
      if record[16] == 8 && pointer(record, 304) != 0 {
        let bvh = resource.read_address(pointer(record, 304), 128).unwrap();
        for offset in [0, 112] {
          if pointer(bvh, offset) != 0 {
            resource.read_address(pointer(bvh, offset), 16).unwrap();
          }
        }
      }
    }
  }
  let resource = super::Rsc7Resource::decode(bytes).unwrap();
  inspect(&resource, super::BASE, true);
  read_ybn_root(bytes).unwrap();
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
  assert_eq!(polygon_count(&actual), polygon_count(&before));
  assert_eq!(polygon_differences(&before, &actual), (0, 0));
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
  let rebuilt = encode_ybn_bound(&original).unwrap();
  let rebuilt_root = read_ybn_root(&rebuilt).unwrap();
  assert_eq!(polygon_count(&rebuilt_root), polygon_count(&original));
  assert_eq!(polygon_differences(&original, &rebuilt_root), (0, 0));
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

  let merged = merge_ybn_deltas(&vanilla, &[&modded], Default::default()).unwrap();
  let root = read_ybn_root(&merged).unwrap();
  let modded_root = read_ybn_root(&modded).unwrap();
  assert_eq!(
    polygon_count(&root),
    2,
    "one vanilla triangle is removed, one survives, and one is added"
  );
  assert_eq!(polygon_differences(&modded_root, &root), (0, 0));
  assert_eq!(root.children.len(), 1, "the added triangle joins the child it shares a corner with");
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

  let merged = merge_ybn_deltas(&vanilla, &[&modded], Default::default()).unwrap();
  let merged_root = read_ybn_root(&merged).unwrap();
  assert_eq!(polygon_count(&merged_root), 2, "one triangle and one replacement box must remain");
  assert_eq!(polygon_differences(&read_ybn_root(&modded).unwrap(), &merged_root), (0, 0));
}

#[test]
fn merges_mod_child_additions_and_removals_against_vanilla() {
  let vanilla = sample("resource_a.ybn.xml");
  let unchanged_mod = sample("resource_a.ybn.xml");
  let changed_mod = sample("resource_b.ybn.xml");
  let merged =
    merge_ybn_deltas(&vanilla, &[&unchanged_mod, &changed_mod], Default::default()).unwrap();
  let (children, minimum, maximum) = merged_children(&merged);

  assert_eq!(children, 1, "the removed vanilla child must not survive");
  assert_eq!(minimum[0], -1.0, "the root broadphase must retain the baseline extent");
  assert_eq!(maximum[0], 11.0);
}

#[test]
fn configured_ybn_tolerance_removes_shifted_boxes_but_preserves_distinct_additions() {
  use crate::core::config::matching::{MatchTolerances, with_tolerances};
  let mut baseline = read_ybn_root(&triangle_root(
    &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
    &[([0, 1, 2], [false; 3])],
  ))
  .unwrap();
  let geometry = baseline.children[0].geometry.as_mut().unwrap();
  geometry.vertices.extend([[0.1, 0.1, 0.0], [0.2, 0.1, 0.0], [0.2, 0.2, 0.0], [0.1, 0.2, 0.0]]);
  geometry.polygons.push(Polygon::Box {
    material: 0,
    vertices: [3, 4, 5, 6],
  });
  let vanilla = encode_ybn_bound(&baseline).unwrap();
  let baseline = read_ybn_root(&vanilla).unwrap();
  let mut deleted = baseline.clone();
  deleted.children[0]
    .geometry
    .as_mut()
    .unwrap()
    .polygons
    .retain(|polygon| !matches!(polygon, Polygon::Box { .. }));
  let deleted = encode_ybn_bound(&deleted).unwrap();
  let shifted = |distance| {
    let mut changed = baseline.clone();
    let geometry = changed.children[0].geometry.as_mut().unwrap();
    let vertices = geometry
      .polygons
      .iter()
      .find_map(|polygon| match polygon {
        Polygon::Box {
          vertices,
          ..
        } => Some(*vertices),
        _ => None,
      })
      .unwrap();
    for index in vertices {
      geometry.vertices[index as usize][0] += distance;
    }
    encode_ybn_bound(&changed).unwrap()
  };
  let count_boxes = |bytes: &[u8]| {
    read_ybn_root(bytes)
      .unwrap()
      .children
      .iter()
      .filter_map(|child| child.geometry.as_ref())
      .map(|geometry| {
        geometry.polygons.iter().filter(|polygon| matches!(polygon, Polygon::Box { .. })).count()
      })
      .sum::<usize>()
  };
  let moved = shifted(0.024);
  for (tolerance, expected) in [(0.005, 1), (0.05, 0)] {
    with_tolerances(
      MatchTolerances {
        ybn_position: tolerance,
        ..Default::default()
      },
      || {
        for mods in [[deleted.as_slice(), moved.as_slice()], [moved.as_slice(), deleted.as_slice()]]
        {
          assert_eq!(
            count_boxes(&merge_ybn_deltas(&vanilla, &mods, Default::default()).unwrap()),
            expected
          );
        }
      },
    );
  }
  let distinct = shifted(0.08);
  with_tolerances(MatchTolerances::default(), || {
    assert_eq!(
      count_boxes(&merge_ybn_deltas(&vanilla, &[&deleted, &distinct], Default::default()).unwrap()),
      1
    );
  });
}

#[test]
fn merges_independent_additions_from_multiple_mods() {
  let vanilla = sample("vanilla_empty.ybn.xml");
  let first_mod = sample("resource_a.ybn.xml");
  let second_mod = sample("resource_b.ybn.xml");
  let merged =
    merge_ybn_deltas(&vanilla, &[&first_mod, &second_mod, &first_mod], Default::default()).unwrap();
  let (children, minimum, maximum) = merged_children(&merged);

  assert_eq!(children, 2, "the same addition from two mods must be deduplicated");
  assert_eq!(minimum[0], -1.0);
  assert_eq!(maximum[0], 11.0);
}

#[test]
fn ybn_deletion_wins_over_polygon_attribute_changes() {
  let vanilla =
    triangle_root(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], &[([0, 1, 2], [true; 3])]);
  let baseline = read_ybn_root(&vanilla).unwrap();
  let mut changed = baseline.clone();
  let geometry = changed.children[0].geometry.as_mut().unwrap();
  geometry.materials[0].colour_index = 23;
  if let Polygon::Triangle(triangle) = &mut geometry.polygons[0] {
    triangle.vertex_flags = [false; 3];
  }
  let changed = encode_ybn_bound(&changed).unwrap();
  let retained =
    read_ybn_root(&merge_ybn_deltas(&vanilla, &[&changed], Default::default()).unwrap()).unwrap();
  assert_eq!(retained.children[0].geometry.as_ref().unwrap().materials[0].colour_index, 23);
  let mut deleted = baseline.clone();
  deleted.children.clear();
  deleted.transforms.clear();
  deleted.flags.clear();
  let deleted = encode_ybn_bound(&deleted).unwrap();
  for mods in [[deleted.as_slice(), changed.as_slice()], [changed.as_slice(), deleted.as_slice()]] {
    let result =
      read_ybn_root(&merge_ybn_deltas(&vanilla, &mods, Default::default()).unwrap()).unwrap();
    assert!(result.children.is_empty(), "an attribute change must not resurrect a deleted face");
  }
}

#[test]
fn ybn_deletion_matches_box_volume_across_reference_vertex_changes() {
  let mut baseline = read_ybn_root(&triangle_root(
    &[[-1.0, -1.0, -1.0], [1.0, 1.0, -1.0], [-1.0, 1.0, 1.0], [1.0, -1.0, 1.0]],
    &[],
  ))
  .unwrap();
  baseline.children[0].geometry.as_mut().unwrap().polygons = vec![Polygon::Box {
    material: 0,
    vertices: [0, 1, 2, 3],
  }];
  let vanilla = encode_ybn_bound(&baseline).unwrap();
  let mut changed = read_ybn_root(&vanilla).unwrap();
  let geometry = changed.children[0].geometry.as_mut().unwrap();
  for vertex in &mut geometry.vertices {
    for coordinate in vertex {
      *coordinate = -*coordinate;
    }
  }
  geometry.materials[0].colour_index = 23;
  let changed = encode_ybn_bound(&changed).unwrap();
  let mut distinct = read_ybn_root(&changed).unwrap();
  for vertex in &mut distinct.children[0].geometry.as_mut().unwrap().vertices {
    vertex[0] += 10.0;
  }
  let distinct = encode_ybn_bound(&distinct).unwrap();
  let mut deleted = baseline;
  deleted.children.clear();
  deleted.transforms.clear();
  deleted.flags.clear();
  let deleted = encode_ybn_bound(&deleted).unwrap();
  for mods in [[deleted.as_slice(), changed.as_slice()], [changed.as_slice(), deleted.as_slice()]] {
    let result =
      read_ybn_root(&merge_ybn_deltas(&vanilla, &mods, Default::default()).unwrap()).unwrap();
    assert!(result.children.is_empty(), "a re-encoded box must not resurrect a deleted volume");
  }
  let result =
    read_ybn_root(&merge_ybn_deltas(&vanilla, &[&deleted, &distinct], Default::default()).unwrap())
      .unwrap();
  assert_eq!(result.children.len(), 1, "an independent new box must survive");
}

#[test]
#[ignore = "requires local sc1_18_0 vanilla and cached source YBNs named by MLO_SC1_18_0_REENCODED_YBN and MLO_SC1_18_0_DELETING_YBN"]
fn sc1_18_0_deletion_wins_over_a_reencoded_collision() {
  let vanilla = crate::platform::fs::read(
    crate::app::map::vanilla_cache::test_support::asset_latest_file("sc1_18_0.ybn"),
  )
  .unwrap();
  // One source re-encodes the collision; the other deletes polygons from it.
  let reencoded = local_source("MLO_SC1_18_0_REENCODED_YBN");
  let deleting = local_source("MLO_SC1_18_0_DELETING_YBN");
  let baseline = read_ybn_root(&vanilla).unwrap();
  let source = read_ybn_root(&reencoded).unwrap();
  // The source with only the given polygons of its second child.
  let only = |indices: &[usize]| {
    let mut root = source.clone();
    let child = super::diff::legacy::geometry_polygon_subset(
      &root.children[1],
      &indices.iter().copied().collect(),
    )
    .unwrap();
    root.children = vec![child];
    root.transforms = vec![source.transforms[1]];
    root.flags = vec![source.flags[1]];
    root
  };
  assert_eq!(polygon_differences(&only(&[138]), &baseline).0, 0, "box volume must match vanilla");
  let returning = [109, 110, 111, 138, 104, 139];
  for mods in
    [[reencoded.as_slice(), deleting.as_slice()], [deleting.as_slice(), reencoded.as_slice()]]
  {
    let result =
      read_ybn_root(&merge_ybn_deltas(&vanilla, &mods, Default::default()).unwrap()).unwrap();
    assert_eq!(
      polygon_differences(&only(&returning), &result).0,
      returning.len(),
      "re-encoded polygons must not return"
    );
    assert!(polygon_count(&result) > 0, "unrelated collision must remain");
  }
}

#[test]
#[ignore = "requires local Mirror Park vanilla and cached source YBNs named by MLO_KEEPING_YBN and MLO_REMOVING_YBN"]
fn configured_ybn_tolerance_removes_reported_mirrorpark_box() {
  use crate::core::config::matching::{MatchTolerances, with_tolerances};
  let vanilla = crate::platform::fs::read(
    crate::app::map::vanilla_cache::test_support::asset_latest_file("id2_21_c_0.ybn"),
  )
  .unwrap();
  // Two cached source copies of id2_21_c_0.ybn: one keeping the reported box and one removing
  // it.
  let keeping = local_source("MLO_KEEPING_YBN");
  let removing = local_source("MLO_REMOVING_YBN");
  let count = |bytes: &[u8]| {
    let bounds = read_ybn_root(bytes).unwrap();
    bounds
      .children
      .iter()
      .map(|child| {
        let Some(geometry) = &child.geometry else {
          return 0;
        };
        geometry
          .polygons
          .iter()
          .filter(|polygon| {
            let Polygon::Box {
              vertices,
              ..
            } = polygon
            else {
              return false;
            };
            let points = vertices.map(|index| {
              let vertex = geometry.vertices[index as usize];
              super::transform_ybn_point(
                std::array::from_fn(|axis| vertex[axis] + geometry.center[axis]),
                child.transform,
              )
            });
            let center: [f32; 3] =
              std::array::from_fn(|axis| points.iter().map(|point| point[axis]).sum::<f32>() / 4.0);
            center
              .into_iter()
              .zip([1211.126, -507.5948, 67.54723])
              .map(|(coordinate, target)| (coordinate - target).powi(2))
              .sum::<f32>()
              < 1.0
          })
          .count()
      })
      .sum::<usize>()
  };
  assert_eq!(count(&vanilla), 1);
  assert_eq!(count(&keeping), 1);
  assert_eq!(count(&removing), 0);
  for (tolerance, expected) in [(0.005, 1), (0.05, 0)] {
    with_tolerances(
      MatchTolerances {
        ybn_position: tolerance,
        ..Default::default()
      },
      || {
        let merged =
          merge_ybn_deltas(&vanilla, &[&keeping, &removing], Default::default()).unwrap();
        let found = count(&merged);
        eprintln!("Mirror Park Box: tolerance={tolerance}, matches={found}");
        assert_eq!(found, expected);
      },
    );
  }
}
