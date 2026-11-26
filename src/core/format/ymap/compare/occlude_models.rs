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

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use struct_diff::ApplyError;

use crate::{
  common::{position::Position, triangle::Triangle},
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

/// Vertices found so far, each corner within `tolerance` per axis of one taken as it.
pub struct VertexIndex {
  tolerance: f32,
  positions: Vec<Position>,
  cells: HashMap<[i64; 3], Vec<usize>>,
}

/// The vertices of a triangle, rotated to start at the least.
pub type TriangleKey = [usize; 3];

impl VertexIndex {
  pub fn new(tolerance: f32) -> Self {
    Self {
      tolerance,
      positions: Vec::new(),
      cells: HashMap::new(),
    }
  }

  fn cell(
    &self,
    position: &Position,
  ) -> [i64; 3] {
    if self.tolerance > 0.0 {
      [position.x, position.y, position.z].map(|value| (value / self.tolerance).floor() as i64)
    } else {
      [position.x, position.y, position.z].map(|value| i64::from(value.to_bits()))
    }
  }

  /// The vertex `position` is: one found before within the tolerance, or a new one.
  pub fn vertex(
    &mut self,
    position: &Position,
  ) -> usize {
    let cell = self.cell(position);
    let reach = if self.tolerance > 0.0 { 1 } else { 0 };
    for dx in -reach..=reach {
      for dy in -reach..=reach {
        for dz in -reach..=reach {
          let near = [cell[0] + dx, cell[1] + dy, cell[2] + dz];
          for &vertex in self.cells.get(&near).into_iter().flatten() {
            if self.same(&self.positions[vertex], position) {
              return vertex;
            }
          }
        }
      }
    }
    self.positions.push(position.clone());
    let vertex = self.positions.len() - 1;
    self.cells.entry(cell).or_default().push(vertex);
    vertex
  }

  fn same(
    &self,
    known: &Position,
    position: &Position,
  ) -> bool {
    if self.tolerance > 0.0 {
      (known.x - position.x).abs() < self.tolerance
        && (known.y - position.y).abs() < self.tolerance
        && (known.z - position.z).abs() < self.tolerance
    } else {
      known == position
    }
  }

  /// Where vertex `vertex` is: the first position taken as it.
  pub fn position(
    &self,
    vertex: usize,
  ) -> &Position {
    &self.positions[vertex]
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
            corner_1: index.position(a).clone(),
            corner_2: index.position(b).clone(),
            corner_3: index.position(c).clone(),
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
/// triangles it takes, an index past `meshes` naming a new model; see the module docs.
fn place(
  added: &[[usize; 3]],
  flags: u32,
  model_flags: &[u32],
  meshes: &[Vec<[usize; 3]>],
) -> Vec<(usize, Vec<[usize; 3]>)> {
  if added.is_empty() {
    return Vec::new();
  }
  let mut owners = HashMap::<usize, Vec<usize>>::new();
  let mut vertices = meshes.iter().map(|_| None).collect::<Vec<Option<HashSet<usize>>>>();
  for (model, mesh) in meshes.iter().enumerate().filter(|(model, _)| model_flags[*model] == flags) {
    for &vertex in mesh.iter().flatten() {
      let list = owners.entry(vertex).or_default();
      if list.last() != Some(&model) {
        list.push(model);
      }
    }
  }
  let mut placed = BTreeMap::<usize, Vec<[usize; 3]>>::new();
  let mut open: Option<(usize, HashSet<usize>)> = None;
  let mut next_new = meshes.len();
  for piece in pieces(added) {
    let piece_vertices = piece.iter().flatten().copied().collect::<HashSet<_>>();
    let mut shared = BTreeMap::<usize, usize>::new();
    for vertex in &piece_vertices {
      for &model in owners.get(vertex).into_iter().flatten() {
        *shared.entry(model).or_default() += 1;
      }
    }
    let mut candidates = shared.into_iter().collect::<Vec<_>>();
    candidates.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let joined = candidates.into_iter().find_map(|(model, _)| {
      let known = vertices[model]
        .get_or_insert_with(|| meshes[model].iter().flatten().copied().collect::<HashSet<_>>());
      let total = known.len() + piece_vertices.difference(known).count();
      (total <= MAX_OCCLUDE_MODEL_VERTICES).then_some(model)
    });
    if let Some(model) = joined {
      vertices[model].as_mut().expect("counted above").extend(&piece_vertices);
      placed.entry(model).or_default().extend(&piece);
      continue;
    }
    for part in split(&piece) {
      let part_vertices = part.iter().flatten().copied().collect::<HashSet<_>>();
      let fits = open.as_ref().is_some_and(|(_, known)| {
        known.len() + part_vertices.difference(known).count() <= MAX_OCCLUDE_MODEL_VERTICES
      });
      if !fits {
        open = Some((next_new, HashSet::new()));
        next_new += 1;
      }
      let (model, known) = open.as_mut().expect("opened above");
      known.extend(&part_vertices);
      placed.entry(*model).or_default().extend(part);
    }
  }
  placed.into_iter().collect()
}

/// The connected pieces of `triangles`, by shared vertices, each in triangle order, ordered by
/// their first triangle.
fn pieces(triangles: &[[usize; 3]]) -> Vec<Vec<[usize; 3]>> {
  let mut parent = HashMap::<usize, usize>::new();
  fn root(
    parent: &mut HashMap<usize, usize>,
    vertex: usize,
  ) -> usize {
    let mut current = vertex;
    while let Some(&next) = parent.get(&current).filter(|&&next| next != current) {
      current = next;
    }
    let found = current;
    let mut current = vertex;
    while current != found {
      let next = parent[&current];
      parent.insert(current, found);
      current = next;
    }
    parent.entry(found).or_insert(found);
    found
  }
  for [a, b, c] in triangles {
    let ra = root(&mut parent, *a);
    for vertex in [b, c] {
      let rv = root(&mut parent, *vertex);
      if rv != ra {
        parent.insert(rv, ra);
      }
    }
  }
  let mut order = Vec::<usize>::new();
  let mut by_root = HashMap::<usize, Vec<[usize; 3]>>::new();
  for triangle in triangles {
    let found = root(&mut parent, triangle[0]);
    let piece = by_root.entry(found).or_insert_with(|| {
      order.push(found);
      Vec::new()
    });
    piece.push(*triangle);
  }
  order.into_iter().map(|found| by_root.remove(&found).unwrap_or_default()).collect()
}

/// `piece`, one connected piece, split into parts of at most [`MAX_OCCLUDE_MODEL_VERTICES`]
/// vertices, taking triangles breadth-first from the first so each part stays connected where
/// it can.
fn split(piece: &[[usize; 3]]) -> Vec<Vec<[usize; 3]>> {
  let total = piece.iter().flatten().copied().collect::<HashSet<_>>().len();
  if total <= MAX_OCCLUDE_MODEL_VERTICES {
    return vec![piece.to_vec()];
  }
  let mut by_vertex = HashMap::<usize, Vec<usize>>::new();
  for (position, triangle) in piece.iter().enumerate() {
    for &vertex in triangle {
      by_vertex.entry(vertex).or_default().push(position);
    }
  }
  let mut visited = vec![false; piece.len()];
  let mut order = Vec::with_capacity(piece.len());
  let mut queue = VecDeque::new();
  for start in 0..piece.len() {
    if visited[start] {
      continue;
    }
    visited[start] = true;
    queue.push_back(start);
    while let Some(position) = queue.pop_front() {
      order.push(position);
      for vertex in piece[position] {
        for &next in &by_vertex[&vertex] {
          if !visited[next] {
            visited[next] = true;
            queue.push_back(next);
          }
        }
      }
    }
  }
  let mut parts = Vec::new();
  let mut part = Vec::new();
  let mut known = HashSet::new();
  for position in order {
    let triangle = piece[position];
    let new = triangle.iter().filter(|vertex| !known.contains(*vertex)).count();
    if known.len() + new > MAX_OCCLUDE_MODEL_VERTICES {
      parts.push(std::mem::take(&mut part));
      known.clear();
    }
    known.extend(triangle);
    part.push(triangle);
  }
  parts.push(part);
  parts
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
