//! Tests of YBN polygon diffs: what counts as the same polygon, and how diffs merge and apply.

use super::polygons::{self, GeometryLimits};
use crate::core::format::ybn::model::{Bound, Geometry, Material, Polygon, Triangle};

const TOLERANCE: f32 = 0.05;
const FLAGS: [u32; 2] = [0x3e, 0x07f3_bec0];
const WEAPON: [u32; 2] = [0x2, 0x0061_0000];

fn geometry(
  vertices: Vec<[f32; 3]>,
  polygons: Vec<Polygon>,
) -> Bound {
  let mut common = vec![0; 112];
  common[16] = 8;
  Bound {
    kind: "GeometryBVH".into(),
    common,
    extension: vec![],
    geometry: Some(Geometry {
      vertices,
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

fn triangle(vertices: [u16; 3]) -> Polygon {
  Polygon::Triangle(Triangle {
    vertices,
    vertex_flags: [false, true, false],
    ..Triangle::default()
  })
}

fn composite(children: Vec<(Bound, [u32; 2])>) -> Bound {
  let mut common = vec![0; 112];
  common[16] = 10;
  let identity = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];
  Bound {
    kind: "Composite".into(),
    common,
    extension: vec![],
    geometry: None,
    flags: children.iter().map(|(_, flags)| *flags).collect(),
    transforms: vec![identity; children.len()],
    children: children.into_iter().map(|(child, _)| child).collect(),
    transform: None,
    composite_flags: [0; 2],
  }
}

/// A strip of `count` triangles along X from `x`, each sharing an edge with the next.
fn strip(
  x: f32,
  count: usize,
) -> Bound {
  let vertices = (0..count + 2)
    .map(|index| [x + (index / 2) as f32, (index % 2) as f32, 0.0])
    .collect::<Vec<_>>();
  let polygons =
    (0..count).map(|index| triangle([index as u16, index as u16 + 1, index as u16 + 2])).collect();
  geometry(vertices, polygons)
}

fn polygon_count(root: &Bound) -> usize {
  root.children.iter().filter_map(|child| child.geometry.as_ref()).map(|g| g.polygons.len()).sum()
}

#[test]
fn indices_rotation_children_and_rounding_change_nothing() {
  let before = composite(vec![(strip(0.0, 4), FLAGS)]);
  // The same triangles from other corners, split over two children, moved within the tolerance.
  let mut first = strip(0.0, 2);
  let mut second = strip(1.0, 2);
  for child in [&mut first, &mut second] {
    let geometry = child.geometry.as_mut().unwrap();
    for polygon in &mut geometry.polygons {
      if let Polygon::Triangle(triangle) = polygon {
        triangle.vertices.rotate_left(1);
        triangle.vertex_flags.rotate_left(1);
      }
    }
    for vertex in &mut geometry.vertices {
      vertex[2] += 0.02;
    }
  }
  let after = composite(vec![(first, FLAGS), (second, FLAGS)]);
  assert!(polygons::diff(&before, &after, TOLERANCE).unwrap().is_empty());
}

#[test]
fn reversed_winding_corner_flags_and_flags_groups_differ() {
  let before = composite(vec![(strip(0.0, 1), FLAGS)]);
  let mut reversed = strip(0.0, 1);
  if let Polygon::Triangle(triangle) = &mut reversed.geometry.as_mut().unwrap().polygons[0] {
    triangle.vertices.swap(1, 2);
    triangle.vertex_flags.swap(1, 2);
  }
  let diff = polygons::diff(&before, &composite(vec![(reversed, FLAGS)]), TOLERANCE).unwrap();
  assert_eq!((diff.polygons[0].removed.len(), diff.polygons[0].added.len()), (1, 1));
  // The same surface in another collision layer.
  let diff = polygons::diff(&before, &composite(vec![(strip(0.0, 1), WEAPON)]), TOLERANCE).unwrap();
  assert_eq!(diff.polygons.iter().map(|group| group.flags).collect::<Vec<_>>(), [WEAPON, FLAGS]);
}

#[test]
fn primitives_match_by_shape_and_radius() {
  let vertices = vec![[0.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 1.0], [1.0, 0.0, 1.0]];
  let shapes = |radius: f32, swap: bool| {
    let (first, second) = if swap { (1, 0) } else { (0, 1) };
    vec![
      Polygon::Sphere {
        material: 0,
        vertex: 0,
        radius,
      },
      Polygon::Capsule {
        material: 0,
        vertex1: first,
        vertex2: second,
        radius,
      },
      Polygon::Cylinder {
        material: 0,
        vertex1: first,
        vertex2: second,
        radius,
      },
      Polygon::Box {
        material: 0,
        vertices: if swap { [1, 0, 3, 2] } else { [0, 1, 2, 3] },
      },
    ]
  };
  let before = composite(vec![(geometry(vertices.clone(), shapes(1.0, false)), FLAGS)]);
  let same = composite(vec![(geometry(vertices.clone(), shapes(1.04, true)), FLAGS)]);
  assert!(polygons::diff(&before, &same, TOLERANCE).unwrap().is_empty());
  let wider = composite(vec![(geometry(vertices, shapes(1.2, false)), FLAGS)]);
  let diff = polygons::diff(&before, &wider, TOLERANCE).unwrap();
  assert_eq!((diff.polygons[0].removed.len(), diff.polygons[0].added.len()), (3, 3));
}

#[test]
fn exact_repeats_count_once_and_are_removed_together() {
  let mut repeated = strip(0.0, 2);
  repeated.geometry.as_mut().unwrap().polygons.push(triangle([0, 1, 2]));
  let mut before = composite(vec![(repeated, FLAGS)]);
  assert!(
    polygons::diff(&before, &composite(vec![(strip(0.0, 2), FLAGS)]), TOLERANCE)
      .unwrap()
      .is_empty()
  );
  let removing =
    polygons::diff(&before, &composite(vec![(strip(1.0, 1), FLAGS)]), TOLERANCE).unwrap();
  polygons::apply(&mut before, &removing, TOLERANCE, GeometryLimits::default()).unwrap();
  assert_eq!(polygon_count(&before), 1);
}

#[test]
fn additions_join_the_child_they_connect_to_or_make_new_ones() {
  let mut root = composite(vec![(strip(0.0, 2), FLAGS), (strip(10.0, 2), FLAGS)]);
  let after =
    composite(vec![(strip(10.0, 4), FLAGS), (strip(0.0, 2), FLAGS), (strip(50.0, 1), FLAGS)]);
  let diff = polygons::diff(&root, &after, TOLERANCE).unwrap();
  polygons::apply(&mut root, &diff, TOLERANCE, GeometryLimits::default()).unwrap();
  let counts = root
    .children
    .iter()
    .map(|child| child.geometry.as_ref().unwrap().polygons.len())
    .collect::<Vec<_>>();
  assert_eq!(counts, [2, 4, 1]);
  assert!(polygons::diff(&root, &after, TOLERANCE).unwrap().is_empty());
}

#[test]
fn a_piece_over_the_limits_splits_into_connected_children() {
  let mut root = composite(vec![(strip(0.0, 1), FLAGS)]);
  let after = composite(vec![(strip(0.0, 1), FLAGS), (strip(100.0, 10), FLAGS)]);
  let diff = polygons::diff(&root, &after, TOLERANCE).unwrap();
  let limits = GeometryLimits {
    max_geometry_vertices: 65534,
    max_geometry_polygons: 4,
  };
  polygons::apply(&mut root, &diff, TOLERANCE, limits).unwrap();
  let counts = root
    .children
    .iter()
    .map(|child| child.geometry.as_ref().unwrap().polygons.len())
    .collect::<Vec<_>>();
  assert_eq!(counts, [1, 4, 4, 2]);
  assert!(polygons::diff(&root, &after, TOLERANCE).unwrap().is_empty());
}

#[test]
fn a_removal_wins_over_another_sources_change_of_the_geometry() {
  let vanilla = composite(vec![(strip(0.0, 2), FLAGS)]);
  let mut recoloured = strip(0.0, 2);
  if let Polygon::Triangle(triangle) = &mut recoloured.geometry.as_mut().unwrap().polygons[0] {
    triangle.vertex_flags = [true; 3];
  }
  let changing =
    polygons::diff(&vanilla, &composite(vec![(recoloured, FLAGS)]), TOLERANCE).unwrap();
  let removing =
    polygons::diff(&vanilla, &composite(vec![(strip(1.0, 1), FLAGS)]), TOLERANCE).unwrap();
  for sources in [[&changing, &removing], [&removing, &changing]] {
    let merged = polygons::merge(&sources, TOLERANCE).unwrap();
    let mut root = vanilla.clone();
    polygons::apply(&mut root, &merged, TOLERANCE, GeometryLimits::default()).unwrap();
    assert_eq!(polygon_count(&root), 1);
  }
}

#[test]
fn equal_additions_of_two_sources_are_placed_once() {
  let vanilla = composite(vec![(strip(0.0, 1), FLAGS)]);
  let adding = composite(vec![(strip(0.0, 1), FLAGS), (strip(20.0, 2), FLAGS)]);
  let diff = polygons::diff(&vanilla, &adding, TOLERANCE).unwrap();
  let merged = polygons::merge(&[&diff, &diff], TOLERANCE).unwrap();
  assert_eq!(merged.polygons[0].added.len(), 2);
}

#[test]
fn removing_a_missing_polygon_is_skipped() {
  let mut root = composite(vec![(strip(0.0, 1), FLAGS)]);
  let other = composite(vec![(strip(30.0, 1), FLAGS)]);
  let removing = polygons::diff(&other, &composite(vec![]), TOLERANCE).unwrap();
  polygons::apply(&mut root, &removing, TOLERANCE, GeometryLimits::default()).unwrap();
  assert_eq!(polygon_count(&root), 1);
}

#[test]
fn limits_are_validated() {
  assert!(GeometryLimits::default().validate().is_ok());
  for limits in [
    GeometryLimits {
      max_geometry_vertices: 65535,
      ..Default::default()
    },
    GeometryLimits {
      max_geometry_polygons: 32768,
      ..Default::default()
    },
    GeometryLimits {
      max_geometry_vertices: 0,
      ..Default::default()
    },
  ] {
    assert!(limits.validate().is_err());
  }
}

#[test]
fn changes_are_carried_to_the_candidates_holding_or_holding_the_box_of_each_polygon() {
  let origin = composite(vec![(strip(0.0, 4), FLAGS), (strip(20.0, 1), FLAGS)]);
  let changed =
    composite(vec![(strip(0.0, 2), FLAGS), (strip(0.1, 1), FLAGS), (strip(1.2, 1), FLAGS)]);
  let diff = polygons::diff(&origin, &changed, TOLERANCE).unwrap();
  // The newer generation splits the strip at x = 1, so its removed half is in the second file.
  let first = composite(vec![(strip(0.0, 2), FLAGS)]);
  let second = composite(vec![(strip(1.0, 2), FLAGS)]);
  let targets =
    [(&first, [[0.0, 0.0, 0.0], [1.5, 1.0, 0.0]]), (&second, [[1.0, 0.0, 0.0], [3.0, 1.0, 0.0]])];
  let (carried, absent) = polygons::carried_among(&diff, &targets, TOLERANCE).unwrap();
  // The strip at x = 20 is in neither.
  assert_eq!(absent, 1);
  let counts = |fields: &polygons::YbnDiffFields| {
    let group = &fields.polygons[0];
    (group.removed.len(), group.added.len())
  };
  // The first holds the addition at x = 0.1; the second the removed half and the addition at
  // x = 1.2, which the first's box does not hold.
  assert_eq!(counts(&carried[0]), (0, 1));
  assert_eq!(counts(&carried[1]), (2, 1));
  let mut root = second.clone();
  polygons::apply(&mut root, &carried[1], TOLERANCE, GeometryLimits::default()).unwrap();
  assert_eq!(polygon_count(&root), 1);

  // An addition no box holds goes to the nearest.
  let far = polygons::diff(
    &origin,
    &composite(vec![(strip(0.0, 4), FLAGS), (strip(20.0, 1), FLAGS), (strip(9.0, 1), FLAGS)]),
    TOLERANCE,
  )
  .unwrap();
  let (carried, absent) = polygons::carried_among(&far, &targets, TOLERANCE).unwrap();
  assert_eq!(absent, 0);
  assert!(carried[0].is_empty());
  assert_eq!(counts(&carried[1]), (0, 1));
}

#[test]
fn repeats_of_one_flags_group_are_counted_and_left_out() {
  // The first child repeats its first triangle; the second repeats the first child's; the third
  // holds the same triangle under other flags, which is no repeat.
  let mut repeating = strip(0.0, 2);
  repeating.geometry.as_mut().unwrap().polygons.push(triangle([0, 1, 2]));
  let mut root =
    composite(vec![(repeating, FLAGS), (strip(0.0, 1), FLAGS), (strip(0.0, 1), WEAPON)]);
  assert_eq!(polygons::repeats(&root).unwrap(), (5, 2));
  let left_out = polygons::apply_with(
    &mut root,
    &polygons::YbnDiffFields::default(),
    TOLERANCE,
    GeometryLimits::default(),
    true,
  )
  .unwrap();
  assert_eq!(left_out, 2);
  assert_eq!(polygons::repeats(&root).unwrap(), (3, 0));
  // The second child, left empty, is dropped; the other flags keep theirs.
  assert_eq!(root.flags, [FLAGS, WEAPON]);
  assert_eq!(polygon_count(&root), 3);
}

#[test]
fn a_carried_removal_names_the_candidates_polygon_so_it_merges_with_its_own_removal() {
  // The older generation lies 0.01 off the candidate, within the tolerance; a resource removes
  // the same triangle from both.
  let older = composite(vec![(strip(0.01, 2), FLAGS)]);
  let candidate = composite(vec![(strip(0.0, 2), FLAGS)]);
  let from_older =
    polygons::diff(&older, &composite(vec![(strip(0.01, 1), FLAGS)]), TOLERANCE).unwrap();
  let own =
    polygons::diff(&candidate, &composite(vec![(strip(0.0, 1), FLAGS)]), TOLERANCE).unwrap();
  let targets = [(&candidate, [[0.0, 0.0, 0.0], [2.0, 1.0, 0.0]])];
  let (carried, absent) = polygons::carried_among(&from_older, &targets, TOLERANCE).unwrap();
  assert_eq!(absent, 0);
  assert_eq!(carried[0].polygons[0].removed, own.polygons[0].removed);
  let merged = polygons::merge(&[&own, &carried[0]], TOLERANCE).unwrap();
  assert_eq!(merged.polygons[0].removed.len(), 1);
}

#[test]
fn only_the_changes_a_source_makes_to_its_baseline_are_kept() {
  // Vanilla later removed the triangle at 10 and added one at 20; the source, made from the
  // baseline, removed the one at 0 and added one at 30.
  let baseline = composite(vec![(strip(0.0, 1), FLAGS), (strip(10.0, 1), FLAGS)]);
  let latest = composite(vec![(strip(0.0, 1), FLAGS), (strip(20.0, 1), FLAGS)]);
  let source = composite(vec![(strip(10.0, 1), FLAGS), (strip(30.0, 1), FLAGS)]);
  let mut diffs = polygons::diff(&latest, &source, TOLERANCE).unwrap();
  assert_eq!(polygons::change_count(&diffs), 4);
  let from_baseline = polygons::diff(&baseline, &source, TOLERANCE).unwrap();
  polygons::relative_to_baseline(&mut diffs, &from_baseline, TOLERANCE).unwrap();
  let [group] = diffs.polygons.as_slice() else {
    panic!("{diffs:?}");
  };
  let x = |polygons: &[super::polygon::ResolvedPolygon]| {
    polygons.iter().map(|polygon| polygon.shape.positions()[0][0]).collect::<Vec<_>>()
  };
  assert_eq!(x(&group.removed), [0.0]);
  assert_eq!(x(&group.added), [30.0]);
}
