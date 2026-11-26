//! Occlude models compared as the triangles of the whole YMAP, grouped by model flags: the
//! `#[diff(with)]` strategy of `Ymap::occlude_models`.
//!
//! A triangle is its corners in winding order, since the game culls the side its normal
//! `(b - a) x (c - a)` faces away from; rotating the corners keeps it. Corners within
//! `ymap_occlude_vertex_position` per axis are one vertex ([`VertexIndex`]), so triangles compare
//! by the vertices they join ([`TriangleKey`]). No vanilla triangle lies in two models of one
//! YMAP, so a triangle and its flags name the model holding it, and a source regrouping triangles
//! into other models changes nothing. Exact repeats count once.
//!
//! The diff holds, per flags value, the triangles removed and added. Applying removes each
//! removed triangle from its model, warning about and skipping one the YMAP lacks, dropping
//! models left empty, and places the added ones so
//! connected triangles share a model: each connected piece of the additions joins the model of
//! that flags sharing most of its vertices that stays within [`MAX_OCCLUDE_MODEL_VERTICES`], and
//! the other pieces fill new models, a piece over the limit split in breadth-first order. Every
//! model changed takes the exact bounds of its vertices; the others keep theirs.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use struct_diff::ApplyError;

use crate::{
  common::{mesh, position::Position, triangle::Triangle},
  core::{
    config::matching::{self, MatchTolerances},
    format::ymap::model::{YmapOccludeModel, ymap_occlude_model::MAX_OCCLUDE_MODEL_VERTICES},
  },
};

/// The triangle changes of the occlude models with one `flags` value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YmapOccludeTrianglesDiff {
  pub flags: u32,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub removed: Vec<Triangle>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub added: Vec<Triangle>,
}

/// Positions found so far as vertices: a position within the tolerance per axis of one found
/// before, the nearest, is that vertex.
pub struct VertexIndex(mesh::VertexIndex);

/// The vertices of a triangle, rotated to start at the least.
pub type TriangleKey = [usize; 3];

impl VertexIndex {
  pub fn new(tolerance: f32) -> Self {
    Self(mesh::VertexIndex::new(tolerance, false, true))
  }

  /// The vertex `position` is: one found before within the tolerance, or a new one.
  pub fn vertex(
    &mut self,
    position: &Position,
  ) -> usize {
    self.0.vertex([position.x, position.y, position.z])
  }

  /// Where vertex `vertex` is: the first position taken as it.
  pub fn position(
    &self,
    vertex: usize,
  ) -> Position {
    let [x, y, z] = self.0.position(vertex);
    Position {
      x,
      y,
      z,
    }
  }

  /// The vertices of `triangle`'s corners, in winding order.
  pub fn corners(
    &mut self,
    triangle: &Triangle,
  ) -> [usize; 3] {
    [&triangle.corner_1, &triangle.corner_2, &triangle.corner_3].map(|corner| self.vertex(corner))
  }

  /// `triangle`'s key.
  pub fn key(
    &mut self,
    triangle: &Triangle,
  ) -> TriangleKey {
    rotated(self.corners(triangle))
  }
}

/// `corners` rotated to start at the least, keeping their winding.
fn rotated(corners: [usize; 3]) -> TriangleKey {
  let start = (0..3).min_by_key(|&index| corners[index]).unwrap_or(0);
  [corners[start], corners[(start + 1) % 3], corners[(start + 2) % 3]]
}

/// The distinct triangles of `models` per flags value, in model order, with their keys.
fn triangles_by_flags<'a>(
  models: &'a [YmapOccludeModel],
  index: &mut VertexIndex,
) -> BTreeMap<u32, Vec<(TriangleKey, &'a Triangle)>> {
  let mut seen = HashSet::new();
  let mut groups = BTreeMap::<u32, Vec<_>>::new();
  for model in models {
    for triangle in &model.triangles {
      let key = index.key(triangle);
      if seen.insert((model.flags, key)) {
        groups.entry(model.flags).or_default().push((key, triangle));
      }
    }
  }
  groups
}

pub fn diff(
  before: &[YmapOccludeModel],
  after: &[YmapOccludeModel],
  tolerances: &MatchTolerances,
) -> Option<Vec<YmapOccludeTrianglesDiff>> {
  let mut index = VertexIndex::new(tolerances.ymap_occlude_vertex_position);
  let before = triangles_by_flags(before, &mut index);
  let after = triangles_by_flags(after, &mut index);
  let keys = |groups: &BTreeMap<u32, Vec<(TriangleKey, &Triangle)>>, flags| {
    groups.get(&flags).into_iter().flatten().map(|(key, _)| *key).collect::<HashSet<_>>()
  };
  let mut diffs = Vec::new();
  for flags in before.keys().chain(after.keys()).copied().collect::<std::collections::BTreeSet<_>>()
  {
    let (kept, made) = (keys(&before, flags), keys(&after, flags));
    let changed = |groups: &BTreeMap<u32, Vec<(TriangleKey, &Triangle)>>, other: &HashSet<_>| {
      groups
        .get(&flags)
        .into_iter()
        .flatten()
        .filter(|(key, _)| !other.contains(key))
        .map(|(_, triangle)| (*triangle).clone())
        .collect::<Vec<_>>()
    };
    let removed = changed(&before, &made);
    let added = changed(&after, &kept);
    if !removed.is_empty() || !added.is_empty() {
      diffs.push(YmapOccludeTrianglesDiff {
        flags,
        removed,
        added,
      });
    }
  }
  (!diffs.is_empty()).then_some(diffs)
}

pub fn apply(
  models: &mut Vec<YmapOccludeModel>,
  diffs: &[YmapOccludeTrianglesDiff],
) -> Result<(), ApplyError> {
  apply_with(models, diffs, matching::current().ymap_occlude_vertex_position)
}

/// [`apply`] with corners within `tolerance` per axis taken as one vertex.
pub fn apply_with(
  models: &mut Vec<YmapOccludeModel>,
  diffs: &[YmapOccludeTrianglesDiff],
  tolerance: f32,
) -> Result<(), ApplyError> {
  let mut index = VertexIndex::new(tolerance);
  // Each model's triangles with their vertices; kept triangles keep their own corners, added
  // ones take those of the vertices they join.
  let mut meshes = models
    .iter()
    .map(|model| {
      model.triangles.iter().map(|triangle| (index.corners(triangle), triangle.clone())).collect()
    })
    .collect::<Vec<Vec<([usize; 3], Triangle)>>>();
  let mut flags = models.iter().map(|model| model.flags).collect::<Vec<_>>();
  let mut changed = vec![false; models.len()];
  for diff in diffs {
    let mut held = HashMap::<TriangleKey, usize>::new();
    for (model, mesh) in meshes.iter().enumerate().filter(|(model, _)| flags[*model] == diff.flags)
    {
      for (corners, _) in mesh {
        held.entry(rotated(*corners)).or_insert(model);
      }
    }
    let mut removals = vec![HashSet::new(); meshes.len()];
    for triangle in &diff.removed {
      let key = index.key(triangle);
      let Some(model) = held.remove(&key) else {
        tracing::warn!(
          "Occlude triangle {triangle:?} of flags {} to remove is not in the YMAP; skipping it",
          diff.flags
        );
        continue;
      };
      removals[model].insert(key);
      changed[model] = true;
    }
    for (model, removed) in removals.iter().enumerate().filter(|(_, removed)| !removed.is_empty()) {
      meshes[model].retain(|(corners, _)| !removed.contains(&rotated(*corners)));
    }
    let mut added = Vec::new();
    for triangle in &diff.added {
      let corners = index.corners(triangle);
      if held.insert(rotated(corners), usize::MAX).is_none() {
        added.push(corners);
      }
    }
    let vertices = meshes
      .iter()
      .map(|mesh| mesh.iter().map(|(corners, _)| *corners).collect::<Vec<_>>())
      .collect::<Vec<_>>();
    for (model, placed) in place(&added, diff.flags, &flags, &vertices) {
      let placed = placed.into_iter().map(|corners @ [a, b, c]| {
        (
          corners,
          Triangle {
            corner_1: index.position(a),
            corner_2: index.position(b),
            corner_3: index.position(c),
          },
        )
      });
      if model < meshes.len() {
        meshes[model].extend(placed);
        changed[model] = true;
      } else {
        meshes.push(placed.collect());
        flags.push(diff.flags);
        changed.push(true);
      }
    }
  }
  let mut result = Vec::with_capacity(meshes.len());
  for (position, mesh) in meshes.into_iter().enumerate() {
    if position < models.len() && !changed[position] {
      result.push(models[position].clone());
      continue;
    }
    if mesh.is_empty() {
      continue;
    }
    let mut seen = HashSet::new();
    let triangles = mesh
      .into_iter()
      .filter(|(corners, _)| seen.insert(rotated(*corners)))
      .map(|(_, triangle)| triangle)
      .collect();
    let mut model = YmapOccludeModel {
      bmin: Position::default(),
      bmax: Position::default(),
      triangles,
      flags: flags[position],
    };
    model.fit_bounds();
    result.push(model);
  }
  YmapOccludeModel::fit_all(&mut result);
  *models = result;
  Ok(())
}

/// Where the added triangles `added` (as vertices) of flags `flags` go: per model index, the
/// triangles it takes, an index past `meshes` naming a new model; see [`mesh::place`].
fn place(
  added: &[[usize; 3]],
  flags: u32,
  model_flags: &[u32],
  meshes: &[Vec<[usize; 3]>],
) -> Vec<(usize, Vec<[usize; 3]>)> {
  if added.is_empty() {
    return Vec::new();
  }
  let mut existing = meshes
    .iter()
    .zip(model_flags)
    .map(|(mesh, model_flags)| (*model_flags == flags).then(|| mesh::Load::of(mesh)))
    .collect::<Vec<_>>();
  let limits = mesh::Limits {
    vertices: MAX_OCCLUDE_MODEL_VERTICES,
    elements: usize::MAX,
    keys: usize::MAX,
  };
  mesh::place(&mut existing, added.to_vec(), &limits)
    .into_iter()
    .map(|(group, triangles)| match group {
      mesh::Group::Existing(model) => (model, triangles),
      mesh::Group::New(index) => (meshes.len() + index, triangles),
    })
    .collect()
}

/// The triangle changes of every source in `sources`, diffs of the occlude models `vanilla`,
/// combined: every source's removals, and every source's additions once each in source order,
/// leaving out those re-adding a removed triangle.
pub fn merge(
  vanilla: &[YmapOccludeModel],
  sources: &[&[YmapOccludeTrianglesDiff]],
) -> Option<Vec<YmapOccludeTrianglesDiff>> {
  let mut index = VertexIndex::new(matching::current().ymap_occlude_vertex_position);
  for model in vanilla {
    for triangle in &model.triangles {
      index.corners(triangle);
    }
  }
  let mut groups = BTreeMap::<u32, (Vec<Triangle>, Vec<Triangle>)>::new();
  let mut removed_keys = HashSet::new();
  for source in sources {
    for diff in source.iter() {
      let (removed, _) = groups.entry(diff.flags).or_default();
      for triangle in &diff.removed {
        if removed_keys.insert((diff.flags, index.key(triangle))) {
          removed.push(triangle.clone());
        }
      }
    }
  }
  let mut added_keys = HashSet::new();
  for source in sources {
    for diff in source.iter() {
      let (_, added) = groups.entry(diff.flags).or_default();
      for triangle in &diff.added {
        let key = (diff.flags, index.key(triangle));
        if removed_keys.contains(&key) {
          tracing::debug!("    occlude_models: a removal wins over re-adding {triangle:?}");
        } else if added_keys.insert(key) {
          added.push(triangle.clone());
        }
      }
    }
  }
  let diffs = groups
    .into_iter()
    .filter(|(_, (removed, added))| !removed.is_empty() || !added.is_empty())
    .map(|(flags, (removed, added))| YmapOccludeTrianglesDiff {
      flags,
      removed,
      added,
    })
    .collect::<Vec<_>>();
  (!diffs.is_empty()).then_some(diffs)
}
