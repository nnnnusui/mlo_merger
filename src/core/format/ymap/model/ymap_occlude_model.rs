use crate::common::{position::Position, triangle::Triangle};

/// Occlude model definition in YMAP
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapOccludeModel {
  pub bmin: Position,
  pub bmax: Position,
  pub triangles: Vec<Triangle>,
  pub flags: u32,
}

/// The most distinct vertices an occlude model's triangles index.
pub const MAX_OCCLUDE_MODEL_VERTICES: usize = 255;

/// How far a vertex may lie outside an occlude model's bounds, covering their float rounding.
const BOUNDS_TOLERANCE: f32 = 1e-3;

impl YmapOccludeModel {
  /// Sets the bounds to the exact bounds of the triangles' corners.
  pub fn fit_bounds(&mut self) {
    (self.bmin, self.bmax) = exact_bounds(&self.triangles);
  }

  /// The models to deploy in place of this one, or `None` when its bounds hold every vertex
  /// within [`BOUNDS_TOLERANCE`] and its triangles index at most
  /// [`MAX_OCCLUDE_MODEL_VERTICES`] distinct vertices. The triangles split in order into
  /// consecutive runs within the vertex limit, each part with the exact bounds of its vertices
  /// and this model's `flags`.
  pub fn fit(&self) -> Option<Vec<YmapOccludeModel>> {
    let mut parts = Vec::new();
    let mut vertices = Vec::<&Position>::new();
    let mut start = 0;
    for (index, triangle) in self.triangles.iter().enumerate() {
      let corners = [&triangle.corner_1, &triangle.corner_2, &triangle.corner_3];
      let new = corners
        .iter()
        .enumerate()
        .filter(|(i, corner)| !vertices.contains(corner) && !corners[..*i].contains(corner))
        .count();
      if vertices.len() + new > MAX_OCCLUDE_MODEL_VERTICES {
        parts.push(start..index);
        vertices.clear();
        start = index;
      }
      for corner in corners {
        if !vertices.contains(&corner) {
          vertices.push(corner);
        }
      }
    }
    if parts.is_empty() && self.bounds_hold(&vertices) {
      return None;
    }
    parts.push(start..self.triangles.len());
    Some(
      parts
        .into_iter()
        .map(|range| {
          let triangles = self.triangles[range].to_vec();
          let (bmin, bmax) = exact_bounds(&triangles);
          YmapOccludeModel {
            bmin,
            bmax,
            triangles,
            flags: self.flags,
          }
        })
        .collect(),
    )
  }

  /// Fits every model of `models` in place, a split model's parts following one another where
  /// it was, returning the index each fitted model had and the number of models it became.
  pub fn fit_all(models: &mut Vec<YmapOccludeModel>) -> Vec<(usize, usize)> {
    let mut fitted = Vec::new();
    let mut result = Vec::with_capacity(models.len());
    for (index, model) in std::mem::take(models).into_iter().enumerate() {
      match model.fit() {
        Some(parts) => {
          fitted.push((index, parts.len()));
          result.extend(parts);
        }
        None => result.push(model),
      }
    }
    *models = result;
    fitted
  }

  /// Whether the bounds hold every vertex of `vertices` within [`BOUNDS_TOLERANCE`].
  fn bounds_hold(
    &self,
    vertices: &[&Position],
  ) -> bool {
    vertices.iter().all(|vertex| {
      [
        (vertex.x, self.bmin.x, self.bmax.x),
        (vertex.y, self.bmin.y, self.bmax.y),
        (vertex.z, self.bmin.z, self.bmax.z),
      ]
      .into_iter()
      .all(|(value, min, max)| value >= min - BOUNDS_TOLERANCE && value <= max + BOUNDS_TOLERANCE)
    })
  }
}

/// The smallest bounds holding every corner of `triangles`.
fn exact_bounds(triangles: &[Triangle]) -> (Position, Position) {
  let mut corners = triangles
    .iter()
    .flat_map(|triangle| [&triangle.corner_1, &triangle.corner_2, &triangle.corner_3]);
  let Some(first) = corners.next() else {
    return (Position::default(), Position::default());
  };
  corners.fold((first.clone(), first.clone()), |(min, max), corner| {
    (
      Position {
        x: min.x.min(corner.x),
        y: min.y.min(corner.y),
        z: min.z.min(corner.z),
      },
      Position {
        x: max.x.max(corner.x),
        y: max.y.max(corner.y),
        z: max.z.max(corner.z),
      },
    )
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn at(
    x: f32,
    y: f32,
    z: f32,
  ) -> Position {
    Position {
      x,
      y,
      z,
    }
  }

  /// A triangle of three vertices no other `strip` triangle shares, along X from `index`.
  fn strip(index: usize) -> Triangle {
    let x = index as f32;
    Triangle {
      corner_1: at(x, 0.0, 0.0),
      corner_2: at(x, 1.0, 0.0),
      corner_3: at(x, 0.0, 1.0),
    }
  }

  fn model(triangles: Vec<Triangle>) -> YmapOccludeModel {
    let (bmin, bmax) = exact_bounds(&triangles);
    YmapOccludeModel {
      bmin,
      bmax,
      triangles,
      flags: 7,
    }
  }

  #[test]
  fn a_model_in_its_bounds_is_kept() {
    let mut model = model((0..3).map(strip).collect());
    assert_eq!(model.fit(), None);
    model.bmax.x += 12.0;
    assert_eq!(model.fit(), None);
    // Float rounding of vanilla bounds leaves vertices just outside them.
    model.bmin.x += 5e-7;
    assert_eq!(model.fit(), None);
  }

  #[test]
  fn a_triangle_outside_the_bounds_refits_them() {
    let original = model((0..3).map(strip).collect());
    let mut covered = original.clone();
    covered.triangles.push(strip(10));
    covered.bmax.x += 12.0;
    let mut grown = original.clone();
    grown.triangles.push(strip(10));
    assert_eq!(grown.fit(), Some(vec![model(grown.triangles.clone())]));
    assert_eq!(covered.fit(), None);
  }

  #[test]
  fn a_model_over_the_vertex_limit_splits_in_order() {
    // 85 strip triangles index the 255 vertices one model holds.
    let triangles = (0..200).map(strip).collect::<Vec<_>>();
    let parts = model(triangles.clone()).fit().unwrap();
    assert_eq!(
      parts,
      vec![
        model(triangles[..85].to_vec()),
        model(triangles[85..170].to_vec()),
        model(triangles[170..].to_vec()),
      ]
    );
    // Shared vertices count once.
    let fan = (0..300)
      .map(|index| Triangle {
        corner_1: at(0.0, 0.0, 0.0),
        corner_2: at(index as f32, 1.0, 0.0),
        corner_3: at(index as f32 + 1.0, 1.0, 0.0),
      })
      .collect::<Vec<_>>();
    let parts = model(fan.clone()).fit().unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].triangles, fan[..253]);
  }

  #[test]
  fn fitting_every_model_keeps_the_parts_in_place() {
    let small = model(vec![strip(0)]);
    let mut models = vec![small.clone(), model((0..100).map(strip).collect()), small.clone()];
    assert_eq!(YmapOccludeModel::fit_all(&mut models), vec![(1, 2)]);
    assert_eq!(models.len(), 4);
    assert_eq!((&models[0], &models[3]), (&small, &small));
    assert_eq!(models[1].triangles.len() + models[2].triangles.len(), 100);
  }
}
