//! YBN collision compared as the polygons of the whole file, grouped by the collision flags of
//! the geometry child holding them, as YMAP occlude model triangles are grouped by flags.
//!
//! A child's flags pair names what the collision is and what collides with it, so a polygon
//! repeated under other flags is the same surface in another collision layer: groups are kept
//! apart. A polygon is its kind, material (but for its colour), corner flags, radius and
//! positions, corners of two files within the `ybn_position` tolerance being one vertex
//! ([`VertexIndex`]):
//! triangles keep their winding from any corner, boxes compare by their eight corners, capsules
//! and cylinders by their endpoints in either order. Within a group, exact repeats count once. Children holding other than supported polygons, such as primitive Bounds, are compared
//! whole.
//!
//! Applying a diff removes each removed polygon from the child holding it, warning about and
//! skipping one the file lacks, and drops children left empty; it places the added polygons so
//! connected ones share a child: each connected piece joins the child of its group sharing most
//! of its vertices that stays within the [`GeometryLimits`], and the other pieces fill new
//! children after the rest, a piece over the limits split breadth-first. Changed children are
//! rebuilt; the others are kept as they are.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io;

use serde::{Deserialize, Serialize};

use super::{
  legacy::{bound_identity, geometry_polygon_subset, is_polygon_geometry, transform_ybn_point},
  merge_geometry::complete_box_corners,
  polygon::{ResolvedPolygon, ResolvedShape},
  resolve,
};
use crate::common::mesh::{self, VertexIndex};
use crate::core::format::{
  compared::{ComparedFile, PropagationOrigin},
  ybn::model::{Bound, Geometry, Material, Polygon, Triangle},
};

const IDENTITY: [f32; 16] = [1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.];

/// The most vertices and polygons the placement fills a geometry child with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
#[serde(default, deny_unknown_fields)]
pub struct GeometryLimits {
  /// At most 65534, the vertex indices being 16-bit.
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub max_geometry_vertices: u32,
  /// At most 32767, the GeometryBVH indexing polygons with 16-bit signed indices.
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub max_geometry_polygons: u32,
}

impl Default for GeometryLimits {
  fn default() -> Self {
    Self {
      max_geometry_vertices: 65534,
      max_geometry_polygons: 32767,
    }
  }
}

impl GeometryLimits {
  /// Rejects limits outside 1 to 65534 vertices and 1 to 32767 polygons.
  pub fn validate(&self) -> io::Result<()> {
    if !(1..=65534).contains(&self.max_geometry_vertices) {
      return Err(invalid_input("ybn.max_geometry_vertices must be within 1 and 65534"));
    }
    if !(1..=32767).contains(&self.max_geometry_polygons) {
      return Err(invalid_input("ybn.max_geometry_polygons must be within 1 and 32767"));
    }
    Ok(())
  }
}

/// The polygon changes of the geometry children with one flags pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YbnPolygonsDiff {
  /// The children's collision flags: what the collision is, and what collides with it.
  pub flags: [u32; 2],
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub removed: Vec<ResolvedPolygon>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub added: Vec<ResolvedPolygon>,
}

/// A Composite child compared whole, with its slot's transform and flags.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YbnChild {
  pub flags: [u32; 2],
  pub transform: [f32; 16],
  #[cfg_attr(feature = "typescript", specta(type = serde_json::Value))]
  pub bound: Bound,
}

/// How a YBN differs from its baseline.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YbnDiffFields {
  /// Per flags pair whose polygons change, the polygons removed and added.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub polygons: Vec<YbnPolygonsDiff>,
  /// Children compared whole that the baseline has and the source lacks.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub removed_children: Vec<YbnChild>,
  /// Children compared whole that the source adds.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub added_children: Vec<YbnChild>,
}

impl YbnDiffFields {
  pub fn is_empty(&self) -> bool {
    self.polygons.is_empty() && self.removed_children.is_empty() && self.added_children.is_empty()
  }
}

/// Differences of one source YBN from the vanilla YBN of the same name.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YbnComparisonDiff {
  /// Version of the document layout; documents of another version are regenerated.
  pub schema_version: u32,
  /// The YBN the differences lead to: the source YBN.
  pub target: ComparedFile,
  /// The YBN the differences are taken from: the vanilla YBN of the same name.
  pub base: ComparedFile,
  /// The `ybn_position` tolerance the differences were taken with.
  pub tolerance: f32,
  /// A digest of the vanilla versions of the YBN the source was compared with to find its
  /// baseline, when there are several.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub vanilla_versions: Option<String>,
  /// The vanilla version the source differs least from, taken as the one it was made from,
  /// when it is not `base`: `diffs` leave out what the source keeps from it (see
  /// [`relative_to_baseline`]).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  #[cfg_attr(feature = "typescript", specta(optional))]
  pub baseline: Option<ComparedFile>,
  pub diffs: YbnDiffFields,
}

impl YbnComparisonDiff {
  /// The current [`Self::schema_version`].
  pub const SCHEMA_VERSION: u32 = 2;
}

/// Changes a resource makes to a vanilla YBN of a generation the game does not load, carried
/// over to the YBN loaded in its place, which the resource does not ship.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(specta::Type))]
pub struct YbnPropagatedDiff {
  /// Version of the document layout.
  pub schema_version: u32,
  /// The resource whose YBN the changes come from.
  pub resource: String,
  /// The YBN the changes are carried to.
  pub base: ComparedFile,
  /// The resource's YBN and the vanilla YBN its changes were taken from.
  pub origins: Vec<PropagationOrigin>,
  pub diffs: YbnDiffFields,
}

impl YbnPropagatedDiff {
  /// The current [`Self::schema_version`].
  pub const SCHEMA_VERSION: u32 = 1;
}

/// A loaded YBN changes are carried to, with its box: `([min], [max])` in world coordinates.
pub type CarryTarget<'a> = (&'a Bound, [[f32; 3]; 2]);

/// The changes of `diff`, taken from a YBN the game does not load, that reach each of
/// `candidates`, loaded YBNs most covering it first, in their order; and how many removals reach
/// none. A removal reaches every candidate holding the polygon within `tolerance` in the same
/// flags group, naming the candidate's polygon, or the child compared whole. An addition goes to the first candidate whose
/// ground-plane box holds the polygon's, or else to the one whose box is nearest it; a child
/// added whole to the first candidate.
pub fn carried_among(
  diff: &YbnDiffFields,
  candidates: &[CarryTarget<'_>],
  tolerance: f32,
) -> io::Result<(Vec<YbnDiffFields>, usize)> {
  let mut carried = vec![YbnDiffFields::default(); candidates.len()];
  if candidates.is_empty() {
    let removals = diff.polygons.iter().map(|group| group.removed.len()).sum::<usize>()
      + diff.removed_children.len();
    return Ok((carried, removals));
  }
  let mut held = Vec::with_capacity(candidates.len());
  for (bound, _) in candidates {
    let (groups, whole) = groups(bound)?;
    let whole =
      whole.iter().map(|child| bound_identity(&child.bound)).collect::<io::Result<HashSet<_>>>()?;
    held.push((groups, whole));
  }
  let mut absent = 0;
  for group in &diff.polygons {
    let lookups = held
      .iter()
      .map(|(groups, _)| {
        let mut lookup = Lookup::new(tolerance);
        for entry in groups.get(&group.flags).into_iter().flatten() {
          lookup.insert(entry.identity.clone());
        }
        lookup
      })
      .collect::<Vec<_>>();
    let mut removed = vec![Vec::new(); candidates.len()];
    for polygon in &group.removed {
      let identity = Identity::of(polygon);
      let mut found = false;
      for (target, lookup) in lookups.iter().enumerate() {
        // The removal names the candidate's own polygon, as a removal names its baseline's, so
        // that it is the very same as the removals the candidate's own sources make of it.
        if let Some(item) = lookup.find(&identity, |_| true) {
          let entries = held[target].0.get(&group.flags).expect("looked up in this group");
          removed[target].push(entries[item].polygon.clone());
          found = true;
        }
      }
      absent += usize::from(!found);
    }
    let mut added = vec![Vec::new(); candidates.len()];
    for polygon in &group.added {
      added[target_of(polygon, candidates)].push(polygon.clone());
    }
    for (target, (removed, added)) in removed.into_iter().zip(added).enumerate() {
      if !removed.is_empty() || !added.is_empty() {
        carried[target].polygons.push(YbnPolygonsDiff {
          flags: group.flags,
          removed,
          added,
        });
      }
    }
  }
  for child in &diff.removed_children {
    let identity = bound_identity(&child.bound)?;
    let mut found = false;
    for (target, (_, whole)) in held.iter().enumerate() {
      if whole.contains(&identity) {
        carried[target].removed_children.push(child.clone());
        found = true;
      }
    }
    absent += usize::from(!found);
  }
  carried[0].added_children = diff.added_children.clone();
  Ok((carried, absent))
}

/// The index of the candidate an added `polygon` goes to; see [`carried_among`].
fn target_of(
  polygon: &ResolvedPolygon,
  candidates: &[CarryTarget<'_>],
) -> usize {
  let radius = shape_radius(&polygon.shape);
  let mut low = [f32::INFINITY; 2];
  let mut high = [f32::NEG_INFINITY; 2];
  for position in polygon.shape.positions() {
    for axis in 0..2 {
      low[axis] = low[axis].min(position[axis] - radius);
      high[axis] = high[axis].max(position[axis] + radius);
    }
  }
  let holds = |(_, [min, max]): &CarryTarget<'_>| {
    (0..2).all(|axis| min[axis] <= low[axis] && high[axis] <= max[axis])
  };
  if let Some(target) = candidates.iter().position(holds) {
    return target;
  }
  // The distance on the ground plane from the polygon's box to each candidate's.
  let distance = |(_, [min, max]): &CarryTarget<'_>| {
    (0..2)
      .map(|axis| (min[axis] - high[axis]).max(low[axis] - max[axis]).max(0.0))
      .map(|gap| gap * gap)
      .sum::<f32>()
  };
  (0..candidates.len())
    .min_by(|&a, &b| distance(&candidates[a]).total_cmp(&distance(&candidates[b])))
    .unwrap_or(0)
}

/// What a polygon is: its kind, material but for its colour, the corner flags of a triangle,
/// its radius and its positions (box corners completed, capsule and cylinder endpoints either way
/// round). Material colours are left out: tools drop colour tables while keeping the indices
/// into them, and the colours have no bearing on collision.
#[derive(Debug, Clone)]
struct Identity {
  kind: u8,
  material: Material,
  corner_flags: [bool; 3],
  radius: f32,
  points: Vec<[f32; 3]>,
}

/// What an [`Identity`] is in every respect but its positions and radius, which match within a
/// tolerance; corner flags sorted, as triangles match from any corner.
type Signature = (u8, Material, [bool; 3]);

impl Identity {
  fn of(polygon: &ResolvedPolygon) -> Self {
    let mut corner_flags = [false; 3];
    let (points, radius) = match &polygon.shape {
      ResolvedShape::Triangle {
        vertices,
        vertex_flags,
      } => {
        corner_flags = *vertex_flags;
        (vertices.to_vec(), 0.0)
      }
      ResolvedShape::Box {
        vertices,
      } => {
        let mut corners = vertices.to_vec();
        complete_box_corners(&mut corners);
        (corners, 0.0)
      }
      ResolvedShape::Sphere {
        center,
        radius,
      } => (vec![*center], *radius),
      ResolvedShape::Capsule {
        endpoints,
        radius,
      }
      | ResolvedShape::Cylinder {
        endpoints,
        radius,
      } => (endpoints.to_vec(), *radius),
    };
    Self {
      kind: polygon.shape.kind(),
      material: Material {
        colour_index: 0,
        ..polygon.material
      },
      corner_flags,
      radius,
      points,
    }
  }

  /// The identity with material and corner flags left out: where the polygon is.
  fn geometry(&self) -> Self {
    Self {
      material: Material::default(),
      corner_flags: [false; 3],
      ..self.clone()
    }
  }

  fn signature(&self) -> Signature {
    let mut flags = self.corner_flags;
    flags.sort_unstable();
    (self.kind, self.material, flags)
  }

  fn centroid(&self) -> [f32; 3] {
    let count = self.points.len() as f32;
    std::array::from_fn(|axis| self.points.iter().map(|point| point[axis]).sum::<f32>() / count)
  }

  /// Whether `other` is the same polygon within `tolerance` per axis and of radius.
  fn matches(
    &self,
    other: &Self,
    tolerance: f32,
  ) -> bool {
    let close = |first: &[f32; 3], second: &[f32; 3]| {
      (0..3).all(|axis| (first[axis] - second[axis]).abs() <= tolerance)
    };
    if self.signature() != other.signature()
      || (self.radius - other.radius).abs() > tolerance
      || self.points.len() != other.points.len()
    {
      return false;
    }
    let count = self.points.len();
    match self.kind {
      0 => (0..count).any(|shift| {
        (0..count).all(|index| {
          close(&self.points[index], &other.points[(index + shift) % count])
            && self.corner_flags[index] == other.corner_flags[(index + shift) % count]
        })
      }),
      1 => {
        let mut used = vec![false; count];
        self.points.iter().all(|point| {
          match (0..count)
            .find(|&candidate| !used[candidate] && close(point, &other.points[candidate]))
          {
            Some(candidate) => {
              used[candidate] = true;
              true
            }
            None => false,
          }
        })
      }
      _ => {
        (0..count).all(|index| close(&self.points[index], &other.points[index]))
          || (0..count).all(|index| close(&self.points[index], &other.points[count - 1 - index]))
      }
    }
  }

  /// The exact form of the polygon, the same for the very same polygon from any corner.
  fn exact_key(&self) -> Vec<u32> {
    let bits = |point: &[f32; 3]| point.map(f32::to_bits);
    let mut points = self.points.iter().map(bits).collect::<Vec<_>>();
    let mut flags = self.corner_flags.to_vec();
    match self.kind {
      0 => {
        let start = (0..points.len()).min_by_key(|&index| points[index]).unwrap_or(0);
        points.rotate_left(start);
        flags.rotate_left(start);
      }
      _ => points.sort_unstable(),
    }
    let material = &self.material;
    let mut key = vec![
      u32::from(self.kind),
      u32::from(material.kind),
      u32::from(material.procedural_id),
      u32::from(material.room_id),
      u32::from(material.ped_density),
      u32::from(material.flags),
      u32::from(material.unknown),
      self.radius.to_bits(),
    ];
    key.extend(flags.into_iter().map(u32::from));
    key.extend(points.into_iter().flatten());
    key
  }
}

/// Polygons found by [`Identity`]: the very same ones by their exact form, and those within a
/// tolerance by the cell of their centroid, cells twice the tolerance wide.
struct Lookup {
  tolerance: f32,
  items: Vec<Identity>,
  exact: HashMap<Vec<u32>, Vec<usize>>,
  cells: HashMap<[i64; 3], Vec<usize>>,
}

impl Lookup {
  fn new(tolerance: f32) -> Self {
    Self {
      tolerance,
      items: Vec::new(),
      exact: HashMap::new(),
      cells: HashMap::new(),
    }
  }

  fn width(&self) -> f64 {
    f64::from(self.tolerance.max(f32::EPSILON)) * 2.0
  }

  /// Adds `identity`, returning its index.
  fn insert(
    &mut self,
    identity: Identity,
  ) -> usize {
    let index = self.items.len();
    let width = self.width();
    let cell = identity.centroid().map(|value| (f64::from(value) / width).floor() as i64);
    self.exact.entry(identity.exact_key()).or_default().push(index);
    self.cells.entry(cell).or_default().push(index);
    self.items.push(identity);
    index
  }

  /// An item matching `identity` of those `keep` accepts, a very same one first.
  fn find(
    &self,
    identity: &Identity,
    keep: impl Fn(usize) -> bool,
  ) -> Option<usize> {
    if let Some(&index) =
      self.exact.get(&identity.exact_key()).into_iter().flatten().find(|&&index| keep(index))
    {
      return Some(index);
    }
    // A centroid within the tolerance lies in this cell or the next one towards it, per axis.
    let width = self.width();
    let scaled = identity.centroid().map(|value| f64::from(value) / width);
    let cell = scaled.map(|value| value.floor() as i64);
    let step: [i64; 3] =
      std::array::from_fn(|axis| if scaled[axis] - (cell[axis] as f64) < 0.5 { -1 } else { 1 });
    let signature = identity.signature();
    for corner in 0..8 {
      let near: [i64; 3] = std::array::from_fn(|axis| {
        cell[axis] + if corner >> axis & 1 == 1 { step[axis] } else { 0 }
      });
      for &index in self.cells.get(&near).into_iter().flatten() {
        let item = &self.items[index];
        if keep(index) && item.signature() == signature && item.matches(identity, self.tolerance) {
          return Some(index);
        }
      }
    }
    None
  }

  fn contains(
    &self,
    identity: &Identity,
  ) -> bool {
    self.find(identity, |_| true).is_some()
  }

  /// The items that are the very same as `identity`, of those `keep` accepts.
  fn same(
    &self,
    identity: &Identity,
    keep: impl Fn(usize) -> bool,
  ) -> Vec<usize> {
    self
      .exact
      .get(&identity.exact_key())
      .into_iter()
      .flatten()
      .copied()
      .filter(|&index| keep(index))
      .collect()
  }
}

/// A geometry child's polygon, resolved to positions in the Composite's space, with its
/// identity and the vertices it joins.
struct Entry {
  identity: Identity,
  polygon: ResolvedPolygon,
  vertices: Vec<usize>,
}

/// The vertices `polygon` joins, for connecting it with others.
fn joined(
  polygon: &ResolvedPolygon,
  index: &mut VertexIndex,
) -> Vec<usize> {
  polygon.shape.positions().iter().map(|position| index.vertex(*position)).collect()
}

/// `polygon` of `geometry`, resolved, with positions in the Composite's space by `bound`'s
/// transform.
fn world_polygon(
  bound: &Bound,
  geometry: &Geometry,
  polygon: &Polygon,
) -> io::Result<ResolvedPolygon> {
  let mut resolved = resolve::resolved(geometry, polygon)?;
  let place = |position: &mut [f32; 3]| *position = transform_ybn_point(*position, bound.transform);
  match &mut resolved.shape {
    ResolvedShape::Triangle {
      vertices,
      ..
    } => vertices.iter_mut().for_each(place),
    ResolvedShape::Box {
      vertices,
    } => vertices.iter_mut().for_each(place),
    ResolvedShape::Sphere {
      center,
      ..
    } => place(center),
    ResolvedShape::Capsule {
      endpoints,
      ..
    }
    | ResolvedShape::Cylinder {
      endpoints,
      ..
    } => endpoints.iter_mut().for_each(place),
  }
  Ok(resolved)
}

/// The polygons of geometry children, by child index.
type ChildEntries = BTreeMap<usize, Vec<Entry>>;

/// The geometry children's polygons of `root`, per child index, with the vertices they join
/// when `index` is given for children of `linked` flags, and the children compared whole.
fn entries(
  root: &Bound,
  mut index: Option<&mut VertexIndex>,
  linked: &HashSet<[u32; 2]>,
) -> io::Result<(ChildEntries, Vec<usize>)> {
  if root.kind != "Composite" {
    return Err(invalid_data("YBN polygon diffs need a Composite root"));
  }
  let mut children = BTreeMap::new();
  let mut whole = Vec::new();
  for (child_index, child) in root.children.iter().enumerate() {
    if !is_polygon_geometry(child) {
      if child.kind != "None" {
        whole.push(child_index);
      }
      continue;
    }
    let geometry = child.geometry.as_ref().expect("a polygon geometry");
    let links = linked.contains(&child_flags(root, child_index));
    let mut list = Vec::with_capacity(geometry.polygons.len());
    for polygon in &geometry.polygons {
      let polygon = world_polygon(child, geometry, polygon)?;
      list.push(Entry {
        identity: Identity::of(&polygon),
        vertices: index
          .as_deref_mut()
          .filter(|_| links)
          .map(|index| joined(&polygon, index))
          .unwrap_or_default(),
        polygon,
      });
    }
    children.insert(child_index, list);
  }
  Ok((children, whole))
}

/// The distinct polygons of `root` per flags pair, in child order, exact repeats once.
fn groups(root: &Bound) -> io::Result<Grouped> {
  let (children, whole) = entries(root, None, &HashSet::new())?;
  let mut groups = BTreeMap::<[u32; 2], (Lookup, Vec<Entry>)>::new();
  for (child_index, list) in children {
    let flags = child_flags(root, child_index);
    let (seen, kept) = groups.entry(flags).or_insert_with(|| (Lookup::new(0.0), Vec::new()));
    for entry in list {
      if !seen.contains(&entry.identity) {
        seen.insert(entry.identity.clone());
        kept.push(entry);
      }
    }
  }
  let whole = whole.into_iter().map(|child_index| whole_child(root, child_index)).collect();
  Ok((groups.into_iter().map(|(flags, (_, kept))| (flags, kept)).collect(), whole))
}

fn child_flags(
  root: &Bound,
  child_index: usize,
) -> [u32; 2] {
  root.flags.get(child_index).copied().unwrap_or_default()
}

fn whole_child(
  root: &Bound,
  child_index: usize,
) -> YbnChild {
  YbnChild {
    flags: child_flags(root, child_index),
    transform: root.transforms.get(child_index).copied().unwrap_or(IDENTITY),
    bound: root.children[child_index].clone(),
  }
}

/// The entries of `entries` that no entry of `other` matches within `tolerance`.
fn unmatched(
  entries: &[Entry],
  other: &[Entry],
  tolerance: f32,
) -> Vec<ResolvedPolygon> {
  let mut lookup = Lookup::new(tolerance);
  for entry in other {
    lookup.insert(entry.identity.clone());
  }
  entries
    .iter()
    .filter(|entry| !lookup.contains(&entry.identity))
    .map(|entry| entry.polygon.clone())
    .collect()
}

/// How `after` differs from `before`, positions and radii within `tolerance` matching.
pub fn diff(
  before: &Bound,
  after: &Bound,
  tolerance: f32,
) -> io::Result<YbnDiffFields> {
  diff_grouped(&groups(before)?, after, tolerance)
}

/// How each of `afters` differs from `before`, as [`diff`] takes it.
pub fn diff_each(
  before: &Bound,
  afters: &[Bound],
  tolerance: f32,
) -> io::Result<Vec<YbnDiffFields>> {
  let before = groups(before)?;
  afters.iter().map(|after| diff_grouped(&before, after, tolerance)).collect()
}

/// A file's distinct polygons by flags, and its children compared whole.
type Grouped = (BTreeMap<[u32; 2], Vec<Entry>>, Vec<YbnChild>);

fn diff_grouped(
  (before_groups, before_whole): &Grouped,
  after: &Bound,
  tolerance: f32,
) -> io::Result<YbnDiffFields> {
  let (after_groups, after_whole) = groups(after)?;
  let mut polygons = Vec::new();
  let all_flags =
    before_groups.keys().chain(after_groups.keys()).copied().collect::<BTreeSet<[u32; 2]>>();
  for flags in all_flags {
    let (old, new) = (
      before_groups.get(&flags).map_or(&[][..], Vec::as_slice),
      after_groups.get(&flags).map_or(&[][..], Vec::as_slice),
    );
    let removed = unmatched(old, new, tolerance);
    let added = unmatched(new, old, tolerance);
    if !removed.is_empty() || !added.is_empty() {
      polygons.push(YbnPolygonsDiff {
        flags,
        removed,
        added,
      });
    }
  }
  let (removed_children, added_children) = whole_changes(before_whole, &after_whole)?;
  Ok(YbnDiffFields {
    polygons,
    removed_children,
    added_children,
  })
}

/// The children compared whole that `before` has and `after` lacks, and the other way round,
/// counting repeats.
fn whole_changes(
  before: &[YbnChild],
  after: &[YbnChild],
) -> io::Result<(Vec<YbnChild>, Vec<YbnChild>)> {
  let identities = |children: &[YbnChild]| {
    children.iter().map(|child| bound_identity(&child.bound)).collect::<io::Result<Vec<_>>>()
  };
  let (before_keys, after_keys) = (identities(before)?, identities(after)?);
  let take = |from: &[YbnChild], keys: &[Vec<u8>], other: &[Vec<u8>]| {
    let mut remaining = HashMap::<&Vec<u8>, usize>::new();
    for key in other {
      *remaining.entry(key).or_default() += 1;
    }
    from
      .iter()
      .zip(keys)
      .filter(|(_, key)| match remaining.get_mut(key) {
        Some(count) if *count > 0 => {
          *count -= 1;
          false
        }
        _ => true,
      })
      .map(|(child, _)| child.clone())
      .collect::<Vec<_>>()
  };
  Ok((take(before, &before_keys, &after_keys), take(after, &after_keys, &before_keys)))
}

/// Leaves out of `latest`, a source's differences from the latest vanilla version, what the
/// source keeps from its baseline, given its differences from the baseline, `from_baseline`:
/// a polygon or child the source removes is kept only when the baseline has it too, and one it
/// adds only when the baseline lacks it too, polygons matching within `tolerance`. So the
/// removals of what vanilla added after the baseline, and the additions of what it removed
/// after, are left out (see [`crate::core::format::ymap::compare::baseline`]).
pub fn relative_to_baseline(
  latest: &mut YbnDiffFields,
  from_baseline: &YbnDiffFields,
  tolerance: f32,
) -> io::Result<()> {
  let lookup = |polygons: &[ResolvedPolygon]| {
    let mut lookup = Lookup::new(tolerance);
    for polygon in polygons {
      lookup.insert(Identity::of(polygon));
    }
    lookup
  };
  for group in &mut latest.polygons {
    let baseline = from_baseline.polygons.iter().find(|baseline| baseline.flags == group.flags);
    let (removed, added) = baseline.map_or((&[][..], &[][..]), |baseline| {
      (baseline.removed.as_slice(), baseline.added.as_slice())
    });
    let (removed, added) = (lookup(removed), lookup(added));
    group.removed.retain(|polygon| removed.contains(&Identity::of(polygon)));
    group.added.retain(|polygon| added.contains(&Identity::of(polygon)));
  }
  latest.polygons.retain(|group| !group.removed.is_empty() || !group.added.is_empty());
  let shared = |children: &mut Vec<YbnChild>, baseline: &[YbnChild]| -> io::Result<()> {
    let mut remaining = HashMap::<Vec<u8>, usize>::new();
    for child in baseline {
      *remaining.entry(bound_identity(&child.bound)?).or_default() += 1;
    }
    let mut kept = Vec::new();
    for child in children.drain(..) {
      if let Some(count) = remaining.get_mut(&bound_identity(&child.bound)?)
        && *count > 0
      {
        *count -= 1;
        kept.push(child);
      }
    }
    *children = kept;
    Ok(())
  };
  shared(&mut latest.removed_children, &from_baseline.removed_children)?;
  shared(&mut latest.added_children, &from_baseline.added_children)?;
  Ok(())
}

/// How many changes `diff` makes, by which the vanilla version a source differs least from is
/// told: one per polygon and per child.
pub fn change_count(diff: &YbnDiffFields) -> usize {
  diff.polygons.iter().map(|group| group.removed.len() + group.added.len()).sum::<usize>()
    + diff.removed_children.len()
    + diff.added_children.len()
}

/// The changes of every source in `sources`, diffs of one baseline, combined: every source's
/// removals, and every source's additions once each in source order, but those placing a
/// geometry that a source removes without placing it again, so a removal wins over another
/// source's material or corner-flag change.
pub fn merge(
  sources: &[&YbnDiffFields],
  tolerance: f32,
) -> io::Result<YbnDiffFields> {
  let mut removed = BTreeMap::<[u32; 2], (Lookup, Vec<ResolvedPolygon>)>::new();
  let mut deleted = BTreeMap::<[u32; 2], Lookup>::new();
  for source in sources {
    for group in &source.polygons {
      let mut placed = Lookup::new(tolerance);
      for polygon in &group.added {
        placed.insert(Identity::of(polygon).geometry());
      }
      // Removals name baseline polygons, so the same ones are the very same.
      let (seen, list) =
        removed.entry(group.flags).or_insert_with(|| (Lookup::new(0.0), Vec::new()));
      for polygon in &group.removed {
        let identity = Identity::of(polygon);
        if !placed.contains(&identity.geometry()) {
          deleted
            .entry(group.flags)
            .or_insert_with(|| Lookup::new(tolerance))
            .insert(identity.geometry());
        }
        if !seen.contains(&identity) {
          seen.insert(identity);
          list.push(polygon.clone());
        }
      }
    }
  }
  // Additions another source makes too, within the tolerance, are placed once; one source's
  // own additions are all placed.
  let mut added = BTreeMap::<[u32; 2], (Lookup, Vec<ResolvedPolygon>)>::new();
  let readded = removed
    .iter()
    .map(|(flags, (seen, _))| {
      let mut lookup = Lookup::new(0.0);
      for identity in &seen.items {
        lookup.insert(identity.clone());
      }
      (*flags, lookup)
    })
    .collect::<BTreeMap<_, _>>();
  for source in sources {
    for group in &source.polygons {
      let (seen, list) =
        added.entry(group.flags).or_insert_with(|| (Lookup::new(tolerance), Vec::new()));
      let earlier = seen.items.len();
      for polygon in &group.added {
        let identity = Identity::of(polygon);
        if deleted.get(&group.flags).is_some_and(|deleted| deleted.contains(&identity.geometry())) {
          tracing::debug!("    YBN polygons: a removal wins over another change of {polygon:?}");
        } else if readded.get(&group.flags).is_some_and(|readded| readded.contains(&identity)) {
          tracing::debug!("    YBN polygons: {polygon:?} re-adds a removed polygon");
        } else if seen.find(&identity, |item| item < earlier).is_none() {
          seen.insert(identity);
          list.push(polygon.clone());
        }
      }
    }
  }
  let mut polygons = Vec::new();
  for flags in removed.keys().chain(added.keys()).copied().collect::<BTreeSet<_>>() {
    let removed = removed.remove(&flags).map(|(_, list)| list).unwrap_or_default();
    let added = added.remove(&flags).map(|(_, list)| list).unwrap_or_default();
    if !removed.is_empty() || !added.is_empty() {
      polygons.push(YbnPolygonsDiff {
        flags,
        removed,
        added,
      });
    }
  }
  let mut removed_children = Vec::new();
  let mut removed_child_keys = HashSet::new();
  let mut added_children = Vec::new();
  let mut added_child_keys = HashSet::new();
  for source in sources {
    for child in &source.removed_children {
      if removed_child_keys.insert(bound_identity(&child.bound)?) {
        removed_children.push(child.clone());
      }
    }
  }
  for source in sources {
    for child in &source.added_children {
      if added_child_keys.insert(bound_identity(&child.bound)?) {
        added_children.push(child.clone());
      }
    }
  }
  Ok(YbnDiffFields {
    polygons,
    removed_children,
    added_children,
  })
}

/// A geometry child while applying: its polygons, those kept from the original by index and
/// those added, with the vertices of each.
struct Slot {
  flags: [u32; 2],
  /// The original child, or the template a new one copies its record from.
  template: Bound,
  transform: [f32; 16],
  kept: Vec<Kept>,
  added: Vec<Placed>,
  changed: bool,
  original: bool,
}

/// The most materials a geometry child holds, its polygons indexing them with a byte.
const MAX_MATERIALS: usize = 256;

/// A polygon's material, with its colour by value.
type MaterialKey = (Material, Option<[u8; 4]>);

fn material_key(polygon: &ResolvedPolygon) -> MaterialKey {
  (polygon.material, polygon.material_colour)
}

impl Slot {
  /// Whether added polygons can join: a GeometryBVH placed as the Composite is.
  fn joinable(&self) -> bool {
    self.template.kind == "GeometryBVH"
      && self.transform == IDENTITY
      && self.template.transform.is_none_or(|transform| transform == IDENTITY)
  }
}

/// Applies `diff` to `root`, positions and radii within `tolerance` matching, filling children
/// within `limits`; see the module docs.
pub fn apply(
  root: &mut Bound,
  diff: &YbnDiffFields,
  tolerance: f32,
  limits: GeometryLimits,
) -> io::Result<()> {
  apply_with(root, diff, tolerance, limits, false).map(|_| ())
}

/// The polygons of `root` that can be compared, and how many of them repeat an earlier one: the
/// very same polygon in the same flags group.
pub fn repeats(root: &Bound) -> io::Result<(usize, usize)> {
  let (children, _) = entries(root, None, &HashSet::new())?;
  let mut seen = HashMap::<[u32; 2], HashSet<Vec<u32>>>::new();
  let (mut polygons, mut repeats) = (0, 0);
  for (child_index, list) in children {
    let seen = seen.entry(child_flags(root, child_index)).or_default();
    for entry in list {
      polygons += 1;
      repeats += usize::from(!seen.insert(entry.identity.exact_key()));
    }
  }
  Ok((polygons, repeats))
}

/// [`apply`], leaving out of `root` the polygons repeating an earlier one of the same flags
/// group when `leave_out_repeats`; returns how many it left out.
pub fn apply_with(
  root: &mut Bound,
  diff: &YbnDiffFields,
  tolerance: f32,
  limits: GeometryLimits,
  leave_out_repeats: bool,
) -> io::Result<usize> {
  let mut index = VertexIndex::new(tolerance, true, false);
  // Only the children of groups taking additions are linked to them.
  let linked =
    diff.polygons.iter().filter(|group| !group.added.is_empty()).map(|group| group.flags).collect();
  let (children, _) = entries(root, Some(&mut index), &linked)?;
  index.next_layer();
  // Every polygon of the file by group, with the slot and position holding it.
  let mut held = HashMap::<[u32; 2], (Lookup, Vec<(usize, usize)>)>::new();
  let mut slots = Vec::new();
  let mut slot_of_child = HashMap::new();
  for (child_index, list) in children {
    let slot_index = slots.len();
    slot_of_child.insert(child_index, slot_index);
    let flags = child_flags(root, child_index);
    let (lookup, places) =
      held.entry(flags).or_insert_with(|| (Lookup::new(tolerance), Vec::new()));
    let mut kept = Vec::with_capacity(list.len());
    for (position, entry) in list.into_iter().enumerate() {
      lookup.insert(entry.identity);
      places.push((slot_index, position));
      kept.push((position, entry.vertices, material_key(&entry.polygon)));
    }
    slots.push(Slot {
      flags,
      template: root.children[child_index].clone(),
      transform: root.transforms.get(child_index).copied().unwrap_or(IDENTITY),
      kept,
      added: Vec::new(),
      changed: false,
      original: true,
    });
  }
  let mut left_out = 0;
  if leave_out_repeats {
    for (lookup, places) in held.values() {
      let mut seen = HashSet::new();
      let mut repeated = HashMap::<usize, HashSet<usize>>::new();
      for (item, identity) in lookup.items.iter().enumerate() {
        if !seen.insert(identity.exact_key()) {
          let (slot_index, position) = places[item];
          repeated.entry(slot_index).or_default().insert(position);
          left_out += 1;
        }
      }
      for (slot_index, positions) in repeated {
        let slot = &mut slots[slot_index];
        slot.kept.retain(|(position, ..)| !positions.contains(position));
        slot.changed = true;
      }
    }
  }
  for group in &diff.polygons {
    let (lookup, places) =
      held.entry(group.flags).or_insert_with(|| (Lookup::new(tolerance), Vec::new()));
    let mut taken = HashSet::new();
    let mut removals = HashMap::<usize, HashSet<usize>>::new();
    for polygon in &group.removed {
      // The polygon and its exact repeats, or else the one matching it within the tolerance.
      let identity = Identity::of(polygon);
      let mut items = lookup.same(&identity, |item| !taken.contains(&item));
      if items.is_empty() {
        items.extend(lookup.find(&identity, |item| !taken.contains(&item)));
      }
      if items.is_empty() {
        tracing::warn!(
          "YBN polygon {polygon:?} of flags {:?} to remove is not in the file; skipping it",
          group.flags
        );
        continue;
      }
      for item in items {
        taken.insert(item);
        let (slot_index, position) = places[item];
        removals.entry(slot_index).or_default().insert(position);
      }
    }
    for (slot_index, positions) in removals {
      let slot = &mut slots[slot_index];
      slot.kept.retain(|(position, ..)| !positions.contains(position));
      slot.changed = true;
    }
    // An addition the file holds already, as a carried change may find it, is placed once.
    let existing = lookup.items.len();
    let mut added = Vec::new();
    for polygon in &group.added {
      let identity = Identity::of(polygon);
      if lookup.find(&identity, |item| item < existing && !taken.contains(&item)).is_none() {
        lookup.insert(identity);
        places.push((usize::MAX, 0));
        added.push((polygon.clone(), joined(polygon, &mut index)));
      }
    }
    place(&mut slots, group.flags, added, limits)?;
  }
  let rebuilt = rebuild(root, slots, &slot_of_child, &diff.removed_children, &diff.added_children)?;
  // The root's broadphase keeps its extent and covers every child.
  let mut minimum = read_vec3(&root.common, 48)?;
  let mut maximum = read_vec3(&root.common, 32)?;
  for (child_index, child) in root.children.iter().enumerate() {
    for point in child_extent(child, rebuilt.contains(&child_index))? {
      for axis in 0..3 {
        minimum[axis] = minimum[axis].min(point[axis]);
        maximum[axis] = maximum[axis].max(point[axis]);
      }
    }
  }
  let center = std::array::from_fn(|axis| (minimum[axis] + maximum[axis]) * 0.5);
  let radius = (0..3).map(|axis| (maximum[axis] - center[axis]).powi(2)).sum::<f32>().sqrt();
  let radius = radius.max(read_f32(&root.common, 20)?);
  write_vec3(&mut root.common, 48, minimum)?;
  write_vec3(&mut root.common, 32, maximum)?;
  write_vec3(&mut root.common, 64, center)?;
  write_vec3(&mut root.common, 80, center)?;
  write_f32(&mut root.common, 20, radius)?;
  Ok(left_out)
}

/// Points `child` extends to in the Composite's space: those of a `rebuilt` geometry's polygons,
/// widened by their radii, or else the minimum and maximum of its record.
fn child_extent(
  child: &Bound,
  rebuilt: bool,
) -> io::Result<Vec<[f32; 3]>> {
  if child.kind == "None" {
    return Ok(Vec::new());
  }
  if !rebuilt || !is_polygon_geometry(child) {
    return Ok(
      [read_vec3(&child.common, 48)?, read_vec3(&child.common, 32)?]
        .map(|point| transform_ybn_point(point, child.transform))
        .to_vec(),
    );
  }
  let geometry = child.geometry.as_ref().expect("a polygon geometry");
  let mut points = Vec::new();
  for polygon in &geometry.polygons {
    let polygon = world_polygon(child, geometry, polygon)?;
    let radius = shape_radius(&polygon.shape);
    for position in polygon.shape.positions() {
      points.push(position.map(|value| value - radius));
      points.push(position.map(|value| value + radius));
    }
  }
  Ok(points)
}

fn shape_radius(shape: &ResolvedShape) -> f32 {
  match shape {
    ResolvedShape::Sphere {
      radius,
      ..
    }
    | ResolvedShape::Capsule {
      radius,
      ..
    }
    | ResolvedShape::Cylinder {
      radius,
      ..
    } => *radius,
    _ => 0.0,
  }
}

/// Places `added` (the polygons of flags `flags`, with their vertices) into `slots`, the joinable
/// slots of those flags taking them first; see [`mesh::place`].
fn place(
  slots: &mut Vec<Slot>,
  flags: [u32; 2],
  added: Vec<Placed>,
  limits: GeometryLimits,
) -> io::Result<()> {
  if added.is_empty() {
    return Ok(());
  }
  let mut existing = slots
    .iter()
    .map(|slot| {
      (slot.flags == flags && slot.joinable()).then(|| {
        let mut load = mesh::Load::of(&slot.kept);
        for placed in &slot.added {
          load.add(placed);
        }
        load
      })
    })
    .collect::<Vec<_>>();
  let limits = mesh::Limits {
    vertices: limits.max_geometry_vertices as usize,
    elements: limits.max_geometry_polygons as usize,
    keys: MAX_MATERIALS,
  };
  let template = slots
    .iter()
    .find(|slot| slot.flags == flags && slot.joinable())
    .or_else(|| slots.iter().find(|slot| slot.joinable()))
    .map(|slot| slot.template.clone())
    .ok_or_else(|| invalid_data("YBN has no geometry child to make new ones after"))?;
  let first_new = slots.len();
  for (group, polygons) in mesh::place(&mut existing, added, &limits) {
    let slot_index = match group {
      mesh::Group::Existing(slot_index) => slot_index,
      mesh::Group::New(index) => {
        while slots.len() <= first_new + index {
          let mut template = template.clone();
          template.transform = None;
          slots.push(Slot {
            flags,
            template,
            transform: IDENTITY,
            kept: Vec::new(),
            added: Vec::new(),
            changed: true,
            original: false,
          });
        }
        first_new + index
      }
    };
    let slot = &mut slots[slot_index];
    slot.added.extend(polygons);
    slot.changed = true;
  }
  Ok(())
}

/// An added polygon with the vertices it joins.
type Placed = (ResolvedPolygon, Vec<usize>);

impl mesh::MeshElement for Placed {
  type Key = MaterialKey;

  fn vertices(&self) -> &[usize] {
    &self.1
  }

  fn key(&self) -> MaterialKey {
    material_key(&self.0)
  }
}

/// A kept polygon: its position in the original child, its vertices and its material.
type Kept = (usize, Vec<usize>, MaterialKey);

impl mesh::MeshElement for Kept {
  type Key = MaterialKey;

  fn vertices(&self) -> &[usize] {
    &self.1
  }

  fn key(&self) -> MaterialKey {
    self.2
  }
}

/// Writes `slots` back into `root`'s children: unchanged ones as they were, changed ones rebuilt,
/// emptied ones dropped, new ones after the rest; then the children compared whole that
/// `removed` and `added` name. Returns the indices of the rebuilt children.
fn rebuild(
  root: &mut Bound,
  slots: Vec<Slot>,
  slot_of_child: &HashMap<usize, usize>,
  removed: &[YbnChild],
  added: &[YbnChild],
) -> io::Result<HashSet<usize>> {
  let mut rebuilt_children = HashSet::new();
  let mut remaining = HashMap::<Vec<u8>, usize>::new();
  for child in removed {
    *remaining.entry(bound_identity(&child.bound)?).or_default() += 1;
  }
  let mut slots = slots.into_iter().map(Some).collect::<Vec<_>>();
  let mut children = Vec::new();
  let mut transforms = Vec::new();
  let mut flags = Vec::new();
  let old_children = std::mem::take(&mut root.children);
  for (child_index, child) in old_children.into_iter().enumerate() {
    let transform = root.transforms.get(child_index).copied().unwrap_or(IDENTITY);
    let child_flags = root.flags.get(child_index).copied().unwrap_or_default();
    match slot_of_child.get(&child_index) {
      Some(&slot_index) => {
        let slot = slots[slot_index].take().expect("each slot once");
        let changed = slot.changed;
        if let Some(bound) = rebuilt(slot)? {
          if changed {
            rebuilt_children.insert(children.len());
          }
          children.push(bound);
          transforms.push(transform);
          flags.push(child_flags);
        }
      }
      None => {
        if child.kind != "None"
          && let Some(count) =
            remaining.get_mut(&bound_identity(&child)?).filter(|count| **count > 0)
        {
          *count -= 1;
          continue;
        }
        children.push(child);
        transforms.push(transform);
        flags.push(child_flags);
      }
    }
  }
  for slot in slots.into_iter().flatten() {
    let (transform, child_flags) = (slot.transform, slot.flags);
    if let Some(bound) = rebuilt(slot)? {
      rebuilt_children.insert(children.len());
      children.push(bound);
      transforms.push(transform);
      flags.push(child_flags);
    }
  }
  for child in added {
    children.push(child.bound.clone());
    transforms.push(child.transform);
    flags.push(child.flags);
  }
  if children.len() > u16::MAX as usize {
    return Err(invalid_data("YBN Composite has too many children"));
  }
  root.children = children;
  root.transforms = transforms;
  root.flags = flags;
  Ok(rebuilt_children)
}

/// The child `slot` stands for: its original when unchanged, otherwise rebuilt from the polygons
/// it keeps and those added; `None` when it holds none.
fn rebuilt(slot: Slot) -> io::Result<Option<Bound>> {
  if !slot.changed {
    return Ok(Some(slot.template));
  }
  if slot.kept.is_empty() && slot.added.is_empty() {
    return Ok(None);
  }
  let mut bound = if slot.original {
    let selected = slot.kept.iter().map(|(position, ..)| *position).collect::<HashSet<_>>();
    geometry_polygon_subset(&slot.template, &selected)?
  } else {
    let mut bound = slot.template.clone();
    let template = bound.geometry.take().unwrap_or_default();
    let positions = slot.added.iter().flat_map(|(polygon, _)| polygon.shape.positions().to_vec());
    let (mut minimum, mut maximum) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for position in positions {
      for axis in 0..3 {
        minimum[axis] = minimum[axis].min(position[axis]);
        maximum[axis] = maximum[axis].max(position[axis]);
      }
    }
    bound.geometry = Some(Geometry {
      center: std::array::from_fn(|axis| (minimum[axis] + maximum[axis]) * 0.5),
      unknown_9c: template.unknown_9c,
      unknown_ac: template.unknown_ac,
      ..Geometry::default()
    });
    bound
  };
  let geometry = bound.geometry.as_mut().expect("a geometry child");
  if !slot.added.is_empty() {
    // Only the materials the kept polygons use count toward the limit placement checked.
    crate::core::format::ybn::binary::compact_polygon_materials(geometry);
    let colours = !geometry.vertex_colours.is_empty()
      || (geometry.vertices.is_empty()
        && slot.added.iter().any(|(polygon, _)| !polygon.vertex_colours.is_empty()));
    let mut builder = GeometryBuilder::new(geometry, colours);
    for (polygon, _) in &slot.added {
      let polygon = builder.append(polygon)?;
      builder.geometry.polygons.push(polygon);
    }
    // A grid holding every vertex placed, which the writer rounds them to.
    geometry.vertex_quantum = Some(match bound.kind.as_str() {
      "GeometryBVH" => {
        crate::core::format::ybn::binary::geometry_bvh_quantum(geometry, &bound.common)?
      }
      _ => crate::core::format::ybn::binary::geometry_quantum(&bound.common)?,
    });
  }
  Ok(Some(bound))
}

/// Appends resolved polygons to a geometry, finding its vertices, materials and colours by value.
struct GeometryBuilder<'a> {
  geometry: &'a mut Geometry,
  colours: bool,
  vertices: HashMap<([u32; 3], [u8; 4]), u16>,
}

impl<'a> GeometryBuilder<'a> {
  fn new(
    geometry: &'a mut Geometry,
    colours: bool,
  ) -> Self {
    let mut vertices = HashMap::new();
    for (index, vertex) in geometry.vertices.iter().enumerate() {
      let colour = geometry.vertex_colours.get(index).copied().unwrap_or_default();
      vertices.entry((vertex.map(f32::to_bits), colour)).or_insert(index as u16);
    }
    Self {
      geometry,
      colours,
      vertices,
    }
  }

  fn vertex(
    &mut self,
    position: [f32; 3],
    colour: Option<[u8; 4]>,
  ) -> io::Result<u16> {
    let local = std::array::from_fn(|axis| position[axis] - self.geometry.center[axis]);
    let colour = if self.colours { colour.unwrap_or([255; 4]) } else { [0; 4] };
    let key = (local.map(f32::to_bits), colour);
    if let Some(index) = self.vertices.get(&key) {
      return Ok(*index);
    }
    let index = u16::try_from(self.geometry.vertices.len())
      .ok()
      .filter(|index| *index < u16::MAX)
      .ok_or_else(|| invalid_data("YBN geometry has too many vertices"))?;
    self.geometry.vertices.push(local);
    if self.colours {
      self.geometry.vertex_colours.push(colour);
    }
    self.vertices.insert(key, index);
    Ok(index)
  }

  fn append(
    &mut self,
    polygon: &ResolvedPolygon,
  ) -> io::Result<Polygon> {
    let mut material = polygon.material;
    if let Some(colour) = polygon.material_colour {
      let index = match self.geometry.material_colours.iter().position(|known| *known == colour) {
        Some(index) => index,
        None => {
          self.geometry.material_colours.push(colour);
          self.geometry.material_colours.len() - 1
        }
      };
      material.colour_index =
        u8::try_from(index).map_err(|_| invalid_data("YBN has too many material colours"))?;
    }
    let material_index = match self.geometry.materials.iter().position(|known| *known == material) {
      Some(index) => index,
      None => {
        self.geometry.materials.push(material);
        self.geometry.materials.len() - 1
      }
    };
    let material =
      u8::try_from(material_index).map_err(|_| invalid_data("YBN has too many materials"))?;
    let colour = |index: usize| polygon.vertex_colours.get(index).copied();
    Ok(match &polygon.shape {
      ResolvedShape::Triangle {
        vertices,
        vertex_flags,
      } => Polygon::Triangle(Triangle {
        material,
        vertices: [
          self.vertex(vertices[0], colour(0))?,
          self.vertex(vertices[1], colour(1))?,
          self.vertex(vertices[2], colour(2))?,
        ],
        vertex_flags: *vertex_flags,
        edge_indices: [u16::MAX; 3],
        ..Triangle::default()
      }),
      ResolvedShape::Box {
        vertices,
      } => Polygon::Box {
        material,
        vertices: [
          self.vertex(vertices[0], colour(0))?,
          self.vertex(vertices[1], colour(1))?,
          self.vertex(vertices[2], colour(2))?,
          self.vertex(vertices[3], colour(3))?,
        ],
      },
      ResolvedShape::Sphere {
        center,
        radius,
      } => Polygon::Sphere {
        material,
        vertex: self.vertex(*center, colour(0))?,
        radius: *radius,
      },
      ResolvedShape::Capsule {
        endpoints,
        radius,
      } => Polygon::Capsule {
        material,
        vertex1: self.vertex(endpoints[0], colour(0))?,
        vertex2: self.vertex(endpoints[1], colour(1))?,
        radius: *radius,
      },
      ResolvedShape::Cylinder {
        endpoints,
        radius,
      } => Polygon::Cylinder {
        material,
        vertex1: self.vertex(endpoints[0], colour(0))?,
        vertex2: self.vertex(endpoints[1], colour(1))?,
        radius: *radius,
      },
    })
  }
}

fn read_vec3(
  bytes: &[u8],
  offset: usize,
) -> io::Result<[f32; 3]> {
  Ok([read_f32(bytes, offset)?, read_f32(bytes, offset + 4)?, read_f32(bytes, offset + 8)?])
}

fn read_f32(
  bytes: &[u8],
  offset: usize,
) -> io::Result<f32> {
  let slice =
    bytes.get(offset..offset + 4).ok_or_else(|| invalid_data("YBN record is too short"))?;
  Ok(f32::from_le_bytes(slice.try_into().expect("four bytes")))
}

fn write_vec3(
  bytes: &mut [u8],
  offset: usize,
  value: [f32; 3],
) -> io::Result<()> {
  for (axis, component) in value.into_iter().enumerate() {
    write_f32(bytes, offset + axis * 4, component)?;
  }
  Ok(())
}

fn write_f32(
  bytes: &mut [u8],
  offset: usize,
  value: f32,
) -> io::Result<()> {
  bytes
    .get_mut(offset..offset + 4)
    .ok_or_else(|| invalid_data("YBN record is too short"))?
    .copy_from_slice(&value.to_le_bytes());
  Ok(())
}

fn invalid_data(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

fn invalid_input(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidInput, message)
}
