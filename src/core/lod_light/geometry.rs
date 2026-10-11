use super::{LightAabb, LightObb, Result};

pub(super) fn influence(
  position: [f64; 3],
  direction: [f64; 3],
  radius: f64,
  light_type: u32,
  outer: u8,
) -> Result<(LightObb, LightAabb)> {
  if !position.iter().chain(direction.iter()).all(|value| value.is_finite())
    || !radius.is_finite()
    || radius < 0.0
  {
    return Err("Nonfinite coordinates or invalid light falloff".into());
  }
  let mut obb = LightObb {
    center: position,
    axes: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    half_extents: [radius; 3],
  };
  match light_type {
    1 => {}
    2 | 4 => {
      let length = direction.iter().map(|value| value * value).sum::<f64>().sqrt();
      if length <= f64::EPSILON {
        return Err("Spot/capsule light has zero direction".into());
      }
      let axis = direction.map(|value| value / length);
      let helper = if axis[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [0.0, 1.0, 0.0] };
      let tangent = cross(helper, axis);
      let length = tangent.iter().map(|value| value * value).sum::<f64>().sqrt();
      let tangent = tangent.map(|value| value / length);
      obb.axes = [tangent, cross(axis, tangent), axis];
      if light_type == 4 {
        obb.half_extents[2] += f64::from(outer) * 0.25;
      } else {
        let angle = f64::from(outer) * std::f64::consts::PI / 255.0;
        let minimum = radius * angle.cos().min(0.0);
        let radial =
          if angle < std::f64::consts::FRAC_PI_2 { radius * angle.sin() } else { radius };
        let offset = (minimum + radius) * 0.5;
        obb.center =
          std::array::from_fn(|coordinate| position[coordinate] + axis[coordinate] * offset);
        obb.half_extents = [radial, radial, (radius - minimum) * 0.5];
      }
    }
    _ => return Err(format!("Unsupported light type {light_type}").into()),
  }
  let extents: [f64; 3] = std::array::from_fn(|coordinate| {
    (0..3).map(|axis| obb.axes[axis][coordinate].abs() * obb.half_extents[axis]).sum()
  });
  let minimum: [f64; 3] =
    std::array::from_fn(|coordinate| obb.center[coordinate] - extents[coordinate]);
  let maximum: [f64; 3] =
    std::array::from_fn(|coordinate| obb.center[coordinate] + extents[coordinate]);
  let aabb = LightAabb {
    min_x: minimum[0],
    min_y: minimum[1],
    min_z: minimum[2],
    max_x: maximum[0],
    max_y: maximum[1],
    max_z: maximum[2],
  };
  Ok((obb, aabb))
}

fn cross(
  left: [f64; 3],
  right: [f64; 3],
) -> [f64; 3] {
  [
    left[1] * right[2] - left[2] * right[1],
    left[2] * right[0] - left[0] * right[2],
    left[0] * right[1] - left[1] * right[0],
  ]
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn point_capsule_and_wide_cone_bounds_are_finite() {
    let (_, point) = influence([2.0, 3.0, 4.0], [0.0; 3], 10.0, 1, 0).unwrap();
    assert_eq!(point.min_x, -8.0);
    let (capsule, _) = influence([0.0; 3], [0.0, 0.0, -1.0], 10.0, 4, 80).unwrap();
    assert_eq!(capsule.half_extents, [10.0, 10.0, 30.0]);
    let (cone, _) = influence([0.0; 3], [0.0, 0.0, 1.0], 10.0, 2, 200).unwrap();
    assert_eq!(cone.half_extents[0], 10.0);
    assert!(cone.center[2] - cone.half_extents[2] < 0.0);
    assert!(serde_json::to_string(&cone).is_ok());
    assert!(influence([0.0; 3], [0.0; 3], 10.0, 2, 30).is_err());
  }

  #[test]
  fn spherical_cone_samples_are_inside_obb_for_narrow_and_wide_angles() {
    for outer in [0, 30, 100, 128, 200, 255] {
      let (obb, _) = influence([0.0; 3], [0.0, 0.0, 1.0], 10.0, 2, outer).unwrap();
      let angle = f64::from(outer) * std::f64::consts::PI / 255.0;
      for step in 0..=32 {
        let polar = angle * f64::from(step) / 32.0;
        for radius in [0.0, 5.0, 10.0] {
          let point = [radius * polar.sin(), 0.0, radius * polar.cos()];
          for axis in 0..3 {
            let projection = (0..3)
              .map(|coordinate| {
                (point[coordinate] - obb.center[coordinate]) * obb.axes[axis][coordinate]
              })
              .sum::<f64>();
            assert!(projection.abs() <= obb.half_extents[axis] + 1e-10);
          }
        }
      }
    }
  }
}
