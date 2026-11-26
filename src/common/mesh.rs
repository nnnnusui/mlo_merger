//! Placing mesh elements (triangles, collision polygons) into bounded groups (occlude models,
//! geometry children) so connected elements share a group, and finding vertices by position.
//!
//! Elements are connected when they share a vertex, named by an index into a [`VertexIndex`].
//! A group holds at most [`Limits::vertices`] distinct vertices, [`Limits::elements`] elements
//! and [`Limits::keys`] distinct keys of its elements (such as materials).

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::hash::Hash;

/// What a group may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Limits {
  pub vertices: usize,
  pub elements: usize,
  pub keys: usize,
}

/// An element placed into groups: the vertices it joins, and the key it counts toward
/// [`Limits::keys`].
pub(crate) trait MeshElement {
  type Key: Clone + Eq + Hash;

  fn vertices(&self) -> &[usize];

  fn key(&self) -> Self::Key;
}

impl MeshElement for [usize; 3] {
  type Key = ();

  fn vertices(&self) -> &[usize] {
    self
  }

  fn key(&self) {}
}

/// What a group holds, as [`Limits`] counts it.
#[derive(Debug, Clone)]
pub(crate) struct Load<K> {
  vertices: HashSet<usize>,
  elements: usize,
  keys: HashSet<K>,
}

impl<K: Clone + Eq + Hash> Default for Load<K> {
  fn default() -> Self {
    Self {
      vertices: HashSet::new(),
      elements: 0,
      keys: HashSet::new(),
    }
  }
}

impl<K: Clone + Eq + Hash> Load<K> {
  /// The load of `elements`.
  pub(crate) fn of<'a, E: MeshElement<Key = K> + 'a>(
    elements: impl IntoIterator<Item = &'a E>
  ) -> Self {
    let mut load = Self::default();
    for element in elements {
      load.add(element);
    }
    load
  }

  pub(crate) fn add<E: MeshElement<Key = K>>(
    &mut self,
    element: &E,
  ) {
    self.vertices.extend(element.vertices().iter().copied());
    self.elements += 1;
    self.keys.insert(element.key());
  }

  /// Whether this load and `other` together stay within `limits`.
  fn fits(
    &self,
    other: &Self,
    limits: &Limits,
  ) -> bool {
    self.vertices.len() + other.vertices.difference(&self.vertices).count() <= limits.vertices
      && self.elements + other.elements <= limits.elements
      && self.keys.len() + other.keys.difference(&self.keys).count() <= limits.keys
  }

  fn extend(
    &mut self,
    other: &Self,
  ) {
    self.vertices.extend(&other.vertices);
    self.elements += other.elements;
    self.keys.extend(other.keys.iter().cloned());
  }
}

/// Where [`place`] puts elements: an existing group by its index, or a new one, numbered from 0
/// in the order they are opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Group {
  Existing(usize),
  New(usize),
}

/// Places `elements` into groups: each connected piece of them joins the existing group sharing
/// most of its vertices that stays within `limits`, the first such by index on a tie, and the
/// other pieces fill new groups in order, a piece over the limits split by
/// [`split_breadth_first`]. `existing` holds the load of each existing group elements may join,
/// `None` for one they may not.
pub(crate) fn place<E: MeshElement>(
  existing: &mut [Option<Load<E::Key>>],
  elements: Vec<E>,
  limits: &Limits,
) -> Vec<(Group, Vec<E>)> {
  let mut owners = HashMap::<usize, Vec<usize>>::new();
  for (group, load) in existing.iter().enumerate() {
    for &vertex in load.iter().flat_map(|load| &load.vertices) {
      owners.entry(vertex).or_default().push(group);
    }
  }
  let mut placed = BTreeMap::<Group, Vec<E>>::new();
  let mut open: Option<(usize, Load<E::Key>)> = None;
  let mut next_new = 0;
  for piece in connected_pieces(elements) {
    let load = Load::of(&piece);
    let mut shared = BTreeMap::<usize, usize>::new();
    for vertex in &load.vertices {
      for &group in owners.get(vertex).into_iter().flatten() {
        *shared.entry(group).or_default() += 1;
      }
    }
    let mut candidates = shared.into_iter().collect::<Vec<_>>();
    candidates.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let joined = candidates
      .into_iter()
      .map(|(group, _)| group)
      .find(|&group| existing[group].as_ref().is_some_and(|existing| existing.fits(&load, limits)));
    if let Some(group) = joined {
      existing[group].as_mut().expect("a joinable group").extend(&load);
      placed.entry(Group::Existing(group)).or_default().extend(piece);
      continue;
    }
    for part in split_breadth_first(piece, limits) {
      let load = Load::of(&part);
      if !open.as_ref().is_some_and(|(_, known)| known.fits(&load, limits)) {
        open = Some((next_new, Load::default()));
        next_new += 1;
      }
      let (group, known) = open.as_mut().expect("opened above");
      known.extend(&load);
      placed.entry(Group::New(*group)).or_default().extend(part);
    }
  }
  placed.into_iter().collect()
}

/// The connected pieces of `elements`, by shared vertices, each in element order, ordered by
/// their first element.
pub(crate) fn connected_pieces<E: MeshElement>(elements: Vec<E>) -> Vec<Vec<E>> {
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
  for element in &elements {
    let Some((&first, rest)) = element.vertices().split_first() else {
      continue;
    };
    let first = root(&mut parent, first);
    for &vertex in rest {
      let other = root(&mut parent, vertex);
      if other != first {
        parent.insert(other, first);
      }
    }
  }
  let mut order = Vec::new();
  let mut by_root = HashMap::<Option<usize>, Vec<E>>::new();
  for element in elements {
    let found = element.vertices().first().map(|&vertex| root(&mut parent, vertex));
    by_root
      .entry(found)
      .or_insert_with(|| {
        order.push(found);
        Vec::new()
      })
      .push(element);
  }
  order.into_iter().map(|found| by_root.remove(&found).unwrap_or_default()).collect()
}

/// `piece` split into parts within `limits`, taking elements breadth-first from the first so
/// each part stays connected where it can.
pub(crate) fn split_breadth_first<E: MeshElement>(
  piece: Vec<E>,
  limits: &Limits,
) -> Vec<Vec<E>> {
  if Load::default().fits(&Load::of(&piece), limits) {
    return vec![piece];
  }
  let mut by_vertex = HashMap::<usize, Vec<usize>>::new();
  for (position, element) in piece.iter().enumerate() {
    for &vertex in element.vertices() {
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
      for vertex in piece[position].vertices() {
        for &next in &by_vertex[vertex] {
          if !visited[next] {
            visited[next] = true;
            queue.push_back(next);
          }
        }
      }
    }
  }
  let mut elements = piece.into_iter().map(Some).collect::<Vec<_>>();
  let mut parts = Vec::new();
  let mut part = Vec::new();
  let mut load = Load::default();
  for position in order {
    let element = elements[position].take().expect("each element once");
    let single = Load::of([&element]);
    if !load.fits(&single, limits) {
      parts.push(std::mem::take(&mut part));
      load = Load::default();
    }
    load.extend(&single);
    part.push(element);
  }
  parts.push(part);
  parts
}

/// Positions found so far as vertices, in layers, one per file compared: a position is the
/// vertex of the same position, or else of one within `tolerance` per axis, the nearest, found in
/// an earlier layer, or also in its own layer when `within_layer` is set.
pub(crate) struct VertexIndex {
  tolerance: f32,
  inclusive: bool,
  within_layer: bool,
  positions: Vec<[f32; 3]>,
  layers: Vec<usize>,
  layer: usize,
  exact: HashMap<[u32; 3], usize>,
  cells: HashMap<[i64; 3], Vec<usize>>,
}

impl VertexIndex {
  /// An index taking positions within `tolerance` per axis as one vertex, or within it or at it
  /// when `inclusive`. A tolerance of zero takes only the same positions.
  pub(crate) fn new(
    tolerance: f32,
    inclusive: bool,
    within_layer: bool,
  ) -> Self {
    Self {
      tolerance,
      inclusive,
      within_layer,
      positions: Vec::new(),
      layers: Vec::new(),
      layer: 0,
      exact: HashMap::new(),
      cells: HashMap::new(),
    }
  }

  /// Starts the layer of the next file.
  pub(crate) fn next_layer(&mut self) {
    self.layer += 1;
  }

  fn cell(
    &self,
    position: [f32; 3],
  ) -> [i64; 3] {
    position.map(|value| (f64::from(value) / f64::from(self.tolerance)).floor() as i64)
  }

  /// The vertex `position` is; see the type docs.
  pub(crate) fn vertex(
    &mut self,
    position: [f32; 3],
  ) -> usize {
    let bits = position.map(f32::to_bits);
    if let Some(&vertex) = self.exact.get(&bits) {
      return vertex;
    }
    let vertex = match (self.tolerance > 0.0).then(|| self.nearest(position)).flatten() {
      Some(vertex) => vertex,
      None => {
        self.positions.push(position);
        self.layers.push(self.layer);
        let vertex = self.positions.len() - 1;
        if self.tolerance > 0.0 {
          self.cells.entry(self.cell(position)).or_default().push(vertex);
        }
        vertex
      }
    };
    self.exact.insert(bits, vertex);
    vertex
  }

  fn nearest(
    &self,
    position: [f32; 3],
  ) -> Option<usize> {
    let cell = self.cell(position);
    let mut nearest: Option<(f32, usize)> = None;
    for dx in -1..=1 {
      for dy in -1..=1 {
        for dz in -1..=1 {
          let near = [cell[0] + dx, cell[1] + dy, cell[2] + dz];
          for &vertex in self.cells.get(&near).into_iter().flatten() {
            if self.layers[vertex] > self.layer
              || (!self.within_layer && self.layers[vertex] == self.layer)
            {
              continue;
            }
            let known = self.positions[vertex];
            let distance =
              (0..3).map(|axis| (known[axis] - position[axis]).abs()).fold(0.0f32, f32::max);
            let close =
              if self.inclusive { distance <= self.tolerance } else { distance < self.tolerance };
            if close && nearest.is_none_or(|(best, _)| distance < best) {
              nearest = Some((distance, vertex));
            }
          }
        }
      }
    }
    nearest.map(|(_, vertex)| vertex)
  }

  /// Where vertex `vertex` is: the first position taken as it.
  pub(crate) fn position(
    &self,
    vertex: usize,
  ) -> [f32; 3] {
    self.positions[vertex]
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const LIMITS: Limits = Limits {
    vertices: 4,
    elements: usize::MAX,
    keys: usize::MAX,
  };

  #[test]
  fn pieces_follow_shared_vertices_in_element_order() {
    let pieces = connected_pieces(vec![[0, 1, 2], [10, 11, 12], [2, 3, 4], [12, 13, 14]]);
    assert_eq!(pieces, [vec![[0, 1, 2], [2, 3, 4]], vec![[10, 11, 12], [12, 13, 14]]]);
  }

  #[test]
  fn a_piece_over_the_limits_splits_breadth_first() {
    let strip = vec![[0, 1, 2], [1, 2, 3], [2, 3, 4], [3, 4, 5]];
    assert_eq!(
      split_breadth_first(strip, &LIMITS),
      [vec![[0, 1, 2], [1, 2, 3]], vec![[2, 3, 4], [3, 4, 5]]]
    );
  }

  #[test]
  fn pieces_join_the_group_sharing_most_vertices_that_fits() {
    let mut existing = vec![
      Some(Load::of(&[[0, 1, 2]])),
      None,
      Some(Load::of(&[[2, 3, 7]])),
      Some(Load::of(&[[9, 9, 9]])),
    ];
    let placed = place(&mut existing, vec![[2, 3, 8], [20, 21, 22], [9, 30, 31]], &LIMITS);
    assert_eq!(
      placed,
      [
        (Group::Existing(2), vec![[2, 3, 8]]),
        (Group::Existing(3), vec![[9, 30, 31]]),
        (Group::New(0), vec![[20, 21, 22]]),
      ]
    );
  }

  #[test]
  fn vertices_merge_across_layers_or_within_one_as_told() {
    let mut across = VertexIndex::new(0.05, true, false);
    let first = across.vertex([0.0, 0.0, 0.0]);
    let second = across.vertex([0.03, 0.0, 0.0]);
    assert_ne!(second, first);
    across.next_layer();
    // The nearest of the earlier layer, the boundary included.
    assert_eq!(across.vertex([0.04, 0.0, 0.0]), second);
    assert_eq!(across.vertex([-0.05, 0.0, 0.0]), first);
    let mut within = VertexIndex::new(0.05, false, true);
    let first = within.vertex([0.0, 0.0, 0.0]);
    assert_eq!(within.vertex([0.03, 0.0, 0.0]), first);
    assert_ne!(within.vertex([0.05, 0.0, 0.0]), first);
  }
}
