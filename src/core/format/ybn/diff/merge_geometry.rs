use super::legacy::points_match;

/// Native boxes store alternating corners; their center reflections complete the volume.
pub(super) fn complete_box_corners(vertices: &mut Vec<[f32; 3]>) {
  let twice_center: [f64; 3] = std::array::from_fn(|axis| {
    vertices.iter().map(|vertex| f64::from(vertex[axis])).sum::<f64>() * 0.5
  });
  for index in 0..4 {
    vertices.push(std::array::from_fn(|axis| {
      (twice_center[axis] - f64::from(vertices[index][axis])) as f32
    }));
  }
}

pub(super) fn corners_match(
  first: &[[f32; 3]],
  second: &[[f32; 3]],
) -> bool {
  fn match_remaining(
    first: &[[f32; 3]],
    second: &[[f32; 3]],
    used: u16,
  ) -> bool {
    let Some((corner, remaining)) = first.split_first() else {
      return true;
    };
    second.iter().enumerate().any(|(index, candidate)| {
      let bit = 1 << index;
      used & bit == 0
        && points_match(*corner, *candidate)
        && match_remaining(remaining, second, used | bit)
    })
  }
  first.len() == second.len() && match_remaining(first, second, 0)
}
