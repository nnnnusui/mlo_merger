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
