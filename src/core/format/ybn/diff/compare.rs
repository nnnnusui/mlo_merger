use super::polygon::{ResolvedPolygon, ResolvedShape};
use std::collections::HashMap;

type Bucket = (Vec<usize>, u8, [i64; 3]);

pub(super) fn bucket(
  path: &[usize],
  polygon: &ResolvedPolygon,
  tolerance: f32,
) -> Bucket {
  let positions = polygon.shape.positions();
  let center: [f64; 3] = std::array::from_fn(|axis| {
    positions.iter().map(|point| f64::from(point[axis])).sum::<f64>() / positions.len() as f64
  });
  (
    path.to_vec(),
    polygon.shape.kind(),
    center.map(|value| (value / (f64::from(tolerance) * 2.0)).floor() as i64),
  )
}

fn point(
  first: &[f32; 3],
  second: &[f32; 3],
  tolerance: f32,
) -> bool {
  first.iter().zip(second).all(|(first, second)| (first - second).abs() <= tolerance)
}

pub(super) fn matches(
  first: &ResolvedPolygon,
  second: &ResolvedPolygon,
  tolerance: f32,
) -> bool {
  if first.material != second.material
    || first.material_colour != second.material_colour
    || first.vertex_colours.len() != second.vertex_colours.len()
  {
    return false;
  }
  let colour = |first_index: usize, second_index: usize| {
    first.vertex_colours.is_empty()
      || first.vertex_colours[first_index] == second.vertex_colours[second_index]
  };
  match (&first.shape, &second.shape) {
    (
      ResolvedShape::Triangle {
        vertices: before,
        vertex_flags: before_flags,
      },
      ResolvedShape::Triangle {
        vertices: after,
        vertex_flags: after_flags,
      },
    ) => (0..3).any(|shift| {
      (0..3).all(|index| {
        point(&before[index], &after[(index + shift) % 3], tolerance)
          && before_flags[index] == after_flags[(index + shift) % 3]
          && colour(index, (index + shift) % 3)
      })
    }),
    (
      ResolvedShape::Box {
        vertices: before,
      },
      ResolvedShape::Box {
        vertices: after,
      },
    ) => {
      (0..4).all(|index| point(&before[index], &after[index], tolerance) && colour(index, index))
    }
    (
      ResolvedShape::Sphere {
        center: before,
        radius: before_radius,
      },
      ResolvedShape::Sphere {
        center: after,
        radius: after_radius,
      },
    ) => {
      point(before, after, tolerance)
        && (before_radius - after_radius).abs() <= tolerance
        && colour(0, 0)
    }
    (
      ResolvedShape::Capsule {
        endpoints: before,
        radius: before_radius,
      },
      ResolvedShape::Capsule {
        endpoints: after,
        radius: after_radius,
      },
    )
    | (
      ResolvedShape::Cylinder {
        endpoints: before,
        radius: before_radius,
      },
      ResolvedShape::Cylinder {
        endpoints: after,
        radius: after_radius,
      },
    ) => {
      (before_radius - after_radius).abs() <= tolerance
        && ((0..2)
          .all(|index| point(&before[index], &after[index], tolerance) && colour(index, index))
          || (0..2).all(|index| {
            point(&before[index], &after[1 - index], tolerance) && colour(index, 1 - index)
          }))
    }
    _ => false,
  }
}

pub(super) fn take_match(
  buckets: &mut HashMap<Bucket, Vec<usize>>,
  records: &[super::resolve::Record],
  path: &[usize],
  polygon: &ResolvedPolygon,
  tolerance: f32,
) -> Option<usize> {
  let (path, kind, cell) = bucket(path, polygon, tolerance);
  for exact in [true, false] {
    for x in -1..=1 {
      for y in -1..=1 {
        for z in -1..=1 {
          let key = (
            path.clone(),
            kind,
            [cell[0].saturating_add(x), cell[1].saturating_add(y), cell[2].saturating_add(z)],
          );
          if let Some(candidates) = buckets.get_mut(&key)
            && let Some(index) = candidates.iter().position(|index| {
              if exact {
                records[*index].polygon == *polygon
              } else {
                matches(&records[*index].polygon, polygon, tolerance)
              }
            })
          {
            return Some(candidates.remove(index));
          }
        }
      }
    }
  }
  None
}
