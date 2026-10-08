use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::io;

use super::{
  compare,
  polygon::{PolygonDiff, ResolvedPolygon, ResolvedShape},
  resolve::{self, BoundMetadata, PrimitiveBounds},
};
use crate::core::format::ybn::model::{Bound, Geometry, Polygon, Triangle};

/// Changes to node metadata, separate from collision primitive additions/removals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum BoundDiff {
  /// A Bounds node introduced in the modified hierarchy.
  Added {
    /// Child slots from the root.
    path: Vec<usize>,
    /// Node metadata, without derived BVH pointers or tables.
    metadata: Box<BoundMetadata>,
  },
  /// A Bounds node removed from the baseline hierarchy.
  Removed {
    /// Child slots from the root.
    path: Vec<usize>,
    /// Previous node metadata.
    metadata: Box<BoundMetadata>,
  },
  /// Changed collision/filter/transform or primitive Bounds metadata.
  Modified {
    /// Child slots from the root.
    path: Vec<usize>,
    /// Baseline metadata.
    before: Box<BoundMetadata>,
    /// Modified metadata.
    after: Box<BoundMetadata>,
  },
}

/// Serializable semantic YBN changes; not a lossless binary or model-reconstruction patch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct YbnDiff {
  /// Coordinate/radius matching tolerance in model units.
  pub tolerance: f32,
  /// Bounds metadata changes with hierarchy ownership retained.
  pub bound_diffs: Vec<BoundDiff>,
  /// Added/removed occurrences, independent of vertex/material/polygon table numbering.
  pub polygon_diffs: Vec<PolygonDiff>,
}

impl YbnDiff {
  /// Extracts changes using the operation's configured collision tolerance.
  ///
  /// ```no_run
  /// let before = mlo_merger::core::format::ybn::read_ybn(&std::fs::read("vanilla.ybn")?)?;
  /// let after = mlo_merger::core::format::ybn::read_ybn(&std::fs::read("mod.ybn")?)?;
  /// let diff = mlo_merger::core::format::ybn::diff::YbnDiff::extract_from(&before, &after)?;
  /// # Ok::<(), std::io::Error>(())
  /// ```
  pub fn extract_from(
    before: &Bound,
    after: &Bound,
  ) -> io::Result<Self> {
    crate::core::config::matching::with_config(|| {
      Self::with_tolerance(before, after, crate::core::config::matching::current().ybn)
    })?
  }

  /// Extracts per-Bounds multiset changes. Ambiguous edits are removed plus added.
  /// Child reordering is conservatively treated by child path, not guessed as node identity.
  pub fn with_tolerance(
    before: &Bound,
    after: &Bound,
    tolerance: f32,
  ) -> io::Result<Self> {
    if !tolerance.is_finite() || tolerance <= 0.0 {
      return Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "YBN diff tolerance must be positive and finite",
      ));
    }
    let (before_bounds, before_records) = resolve::collect(before)?;
    let (after_bounds, after_records) = resolve::collect(after)?;
    let paths: BTreeSet<_> = before_bounds.keys().chain(after_bounds.keys()).collect();
    let bound_diffs = paths
      .into_iter()
      .filter_map(|path| match (before_bounds.get(path), after_bounds.get(path)) {
        (Some(before), Some(after)) if before != after => Some(BoundDiff::Modified {
          path: path.clone(),
          before: Box::new(before.clone()),
          after: Box::new(after.clone()),
        }),
        (Some(before), None) => Some(BoundDiff::Removed {
          path: path.clone(),
          metadata: Box::new(before.clone()),
        }),
        (None, Some(after)) => Some(BoundDiff::Added {
          path: path.clone(),
          metadata: Box::new(after.clone()),
        }),
        _ => None,
      })
      .collect();
    let mut buckets = HashMap::new();
    for (index, record) in before_records.iter().enumerate() {
      buckets
        .entry(compare::bucket(&record.location.bound_path, &record.polygon, tolerance))
        .or_insert_with(Vec::new)
        .push(index);
    }
    let mut consumed = vec![false; before_records.len()];
    let mut additions = Vec::new();
    for record in after_records {
      if let Some(index) = compare::take_match(
        &mut buckets,
        &before_records,
        &record.location.bound_path,
        &record.polygon,
        tolerance,
      ) {
        consumed[index] = true;
      } else {
        additions.push(PolygonDiff::Added {
          location: record.location,
          polygon: record.polygon,
        });
      }
    }
    let mut polygon_diffs = before_records
      .into_iter()
      .enumerate()
      .filter_map(|(index, record)| {
        (!consumed[index]).then_some(PolygonDiff::Removed {
          location: record.location,
          polygon: record.polygon,
        })
      })
      .collect::<Vec<_>>();
    polygon_diffs.extend(additions);
    Ok(Self {
      tolerance,
      bound_diffs,
      polygon_diffs,
    })
  }

  /// Reports whether all supported metadata and primitive occurrences matched.
  pub fn is_empty(&self) -> bool {
    self.bound_diffs.is_empty() && self.polygon_diffs.is_empty()
  }

  /// Applies changes to an existing Bounds hierarchy; hierarchy additions/removals require a full model.
  pub fn apply_to(
    &self,
    before: &Bound,
  ) -> io::Result<Bound> {
    let mut result = before.clone();
    for change in &self.bound_diffs {
      match change {
        BoundDiff::Modified {
          path,
          after,
          ..
        } => apply_metadata(bound_at_path_mut(&mut result, path)?, after)?,
        BoundDiff::Added {
          ..
        }
        | BoundDiff::Removed {
          ..
        } => return Err(invalid_input("YBN Bounds hierarchy changes require a full model")),
      }
    }

    let mut removals: Vec<_> = self
      .polygon_diffs
      .iter()
      .filter_map(|change| match change {
        PolygonDiff::Removed {
          location,
          ..
        } => Some((location.bound_path.as_slice(), location.polygon_index)),
        PolygonDiff::Added {
          ..
        } => None,
      })
      .collect();
    removals.sort_by(|(first_path, first_index), (second_path, second_index)| {
      first_path.cmp(second_path).then_with(|| second_index.cmp(first_index))
    });
    for (path, index) in removals {
      let bound = bound_at_path_mut(&mut result, path)?;
      let geometry =
        bound.geometry.as_mut().ok_or_else(|| invalid_data("YBN polygon owner has no Geometry"))?;
      if index >= geometry.polygons.len() {
        return Err(invalid_data("YBN removed polygon index is out of range"));
      }
      geometry.polygons.remove(index);
    }

    let mut additions: Vec<_> = self
      .polygon_diffs
      .iter()
      .filter_map(|change| match change {
        PolygonDiff::Added {
          location,
          polygon,
        } => Some((location, polygon)),
        PolygonDiff::Removed {
          ..
        } => None,
      })
      .collect();
    additions.sort_by(|(first_location, _), (second_location, _)| {
      first_location
        .bound_path
        .cmp(&second_location.bound_path)
        .then_with(|| first_location.polygon_index.cmp(&second_location.polygon_index))
    });
    for (location, polygon) in additions {
      let bound = bound_at_path_mut(&mut result, &location.bound_path)?;
      let geometry =
        bound.geometry.as_mut().ok_or_else(|| invalid_data("YBN polygon owner has no Geometry"))?;
      if location.polygon_index > geometry.polygons.len() {
        return Err(invalid_data("YBN added polygon index is out of range"));
      }
      let polygon = append_polygon(geometry, polygon)?;
      geometry.polygons.insert(location.polygon_index, polygon);
    }
    Ok(result)
  }

  /// Applies supported changes to a newer vanilla model by matching polygon shapes by value.
  pub fn apply_rebased_to(
    &self,
    latest: &Bound,
  ) -> io::Result<Bound> {
    let mut result = latest.clone();
    for change in &self.bound_diffs {
      match change {
        BoundDiff::Modified {
          path,
          before,
          after,
        } => apply_rebased_metadata(bound_at_path_mut(&mut result, path)?, before, after)?,
        BoundDiff::Added {
          ..
        }
        | BoundDiff::Removed {
          ..
        } => return Err(invalid_input("YBN Bounds hierarchy changes cannot be rebased")),
      }
    }

    let mut removals: Vec<_> = self
      .polygon_diffs
      .iter()
      .filter_map(|change| match change {
        PolygonDiff::Removed {
          location,
          polygon,
        } => Some((location, polygon)),
        PolygonDiff::Added {
          ..
        } => None,
      })
      .collect();
    removals.sort_by(|(first, _), (second, _)| {
      first
        .bound_path
        .cmp(&second.bound_path)
        .then_with(|| first.polygon_index.cmp(&second.polygon_index))
    });
    for (location, removed) in removals {
      let bound = bound_at_path_mut(&mut result, &location.bound_path)?;
      let geometry =
        bound.geometry.as_mut().ok_or_else(|| invalid_data("YBN polygon owner has no Geometry"))?;
      let mut matching = None;
      for (index, polygon) in geometry.polygons.iter().enumerate() {
        let resolved = resolve::resolved(geometry, polygon)?;
        if compare::matches(&resolved, removed, self.tolerance) {
          matching = Some(index);
          break;
        }
      }
      if let Some(index) = matching {
        geometry.polygons.remove(index);
      }
    }

    let mut additions: Vec<_> = self
      .polygon_diffs
      .iter()
      .filter_map(|change| match change {
        PolygonDiff::Added {
          location,
          polygon,
        } => Some((location, polygon)),
        PolygonDiff::Removed {
          ..
        } => None,
      })
      .collect();
    additions.sort_by(|(first, _), (second, _)| {
      first
        .bound_path
        .cmp(&second.bound_path)
        .then_with(|| first.polygon_index.cmp(&second.polygon_index))
    });
    for (location, added) in additions {
      let bound = bound_at_path_mut(&mut result, &location.bound_path)?;
      let geometry =
        bound.geometry.as_mut().ok_or_else(|| invalid_data("YBN polygon owner has no Geometry"))?;
      let mut already_present = false;
      for polygon in &geometry.polygons {
        if compare::matches(&resolve::resolved(geometry, polygon)?, added, self.tolerance) {
          already_present = true;
          break;
        }
      }
      if !already_present {
        let polygon = append_polygon(geometry, added)?;
        geometry.polygons.push(polygon);
      }
    }
    Ok(result)
  }
}

fn apply_rebased_metadata(
  bound: &mut Bound,
  before: &BoundMetadata,
  after: &BoundMetadata,
) -> io::Result<()> {
  if before.kind != after.kind || bound.kind != after.kind {
    return Err(invalid_input("YBN Bounds kind changed and cannot be rebased"));
  }
  let mut merged = resolve::metadata(bound)?;
  if before.transform != after.transform {
    merged.transform = after.transform;
  }
  if before.composite_flags != after.composite_flags {
    merge_array(&mut merged.composite_flags, &before.composite_flags, &after.composite_flags);
  }
  if before.child_transforms != after.child_transforms {
    if before.child_transforms.len() != after.child_transforms.len()
      || merged.child_transforms.len() != before.child_transforms.len()
    {
      return Err(invalid_input("YBN child transform layout changed and cannot be rebased"));
    }
    for index in 0..before.child_transforms.len() {
      merge_array(
        &mut merged.child_transforms[index],
        &before.child_transforms[index],
        &after.child_transforms[index],
      );
    }
  }
  if before.child_flags != after.child_flags {
    if before.child_flags.len() != after.child_flags.len()
      || merged.child_flags.len() != before.child_flags.len()
    {
      return Err(invalid_input("YBN child flag layout changed and cannot be rebased"));
    }
    for index in 0..before.child_flags.len() {
      merge_array(
        &mut merged.child_flags[index],
        &before.child_flags[index],
        &after.child_flags[index],
      );
    }
  }
  if before.margin != after.margin {
    merged.margin = after.margin;
  }
  if before.volume != after.volume {
    merged.volume = after.volume;
  }
  if before.inertia != after.inertia {
    merge_array(&mut merged.inertia, &before.inertia, &after.inertia);
  }
  if before.attributes != after.attributes {
    merge_bytes(&mut merged.attributes, &before.attributes, &after.attributes)?;
  }
  if before.unknown_type != after.unknown_type {
    merged.unknown_type = after.unknown_type;
  }
  if before.geometry_unknowns != after.geometry_unknowns {
    merged.geometry_unknowns = after.geometry_unknowns;
  }
  match (&before.primitive, &after.primitive, &mut merged.primitive) {
    (Some(before), Some(after), Some(merged)) => merge_primitive(merged, before, after),
    (None, None, _) => {}
    _ => return Err(invalid_input("YBN primitive kind changed and cannot be rebased")),
  }
  apply_metadata(bound, &merged)
}

fn merge_primitive(
  merged: &mut PrimitiveBounds,
  before: &PrimitiveBounds,
  after: &PrimitiveBounds,
) {
  if before.minimum != after.minimum {
    merge_array(&mut merged.minimum, &before.minimum, &after.minimum);
  }
  if before.maximum != after.maximum {
    merge_array(&mut merged.maximum, &before.maximum, &after.maximum);
  }
  if before.box_center != after.box_center {
    merge_array(&mut merged.box_center, &before.box_center, &after.box_center);
  }
  if before.sphere_center != after.sphere_center {
    merge_array(&mut merged.sphere_center, &before.sphere_center, &after.sphere_center);
  }
  if before.sphere_radius != after.sphere_radius {
    merged.sphere_radius = after.sphere_radius;
  }
  if before.extension != after.extension {
    merged.extension.clone_from(&after.extension);
  }
}

fn merge_array<T: Copy + PartialEq>(
  merged: &mut [T],
  before: &[T],
  after: &[T],
) {
  for index in 0..before.len().min(after.len()).min(merged.len()) {
    if before[index] != after[index] {
      merged[index] = after[index];
    }
  }
}

fn merge_bytes(
  merged: &mut [u8],
  before: &[u8],
  after: &[u8],
) -> io::Result<()> {
  if before.len() != after.len() || merged.len() != before.len() {
    return Err(invalid_input("YBN attribute layout changed and cannot be rebased"));
  }
  for index in 0..before.len() {
    if before[index] != after[index] {
      merged[index] = after[index];
    }
  }
  Ok(())
}

fn invalid_data(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message)
}

fn invalid_input(message: &str) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn bound_at_path_mut<'a>(
  root: &'a mut Bound,
  path: &[usize],
) -> io::Result<&'a mut Bound> {
  let mut bound = root;
  for index in path {
    bound = bound
      .children
      .get_mut(*index)
      .ok_or_else(|| invalid_data("YBN Bounds path is out of range"))?;
  }
  Ok(bound)
}

fn write_f32(
  bytes: &mut [u8],
  offset: usize,
  value: f32,
) -> io::Result<()> {
  let target =
    bytes.get_mut(offset..offset + 4).ok_or_else(|| invalid_data("Truncated YBN Bounds record"))?;
  target.copy_from_slice(&value.to_le_bytes());
  Ok(())
}

fn write_vec3(
  bytes: &mut [u8],
  offset: usize,
  value: [f32; 3],
) -> io::Result<()> {
  for (axis, value) in value.into_iter().enumerate() {
    write_f32(bytes, offset + axis * 4, value)?;
  }
  Ok(())
}

fn apply_metadata(
  bound: &mut Bound,
  metadata: &BoundMetadata,
) -> io::Result<()> {
  if bound.kind != metadata.kind || bound.common.len() < 112 || metadata.attributes.len() != 6 {
    return Err(invalid_input("YBN Bounds kind or metadata layout changed"));
  }
  bound.transform = metadata.transform;
  bound.composite_flags = metadata.composite_flags;
  bound.transforms.clone_from(&metadata.child_transforms);
  bound.flags.clone_from(&metadata.child_flags);
  write_f32(&mut bound.common, 44, metadata.margin)?;
  write_vec3(&mut bound.common, 96, metadata.inertia)?;
  write_f32(&mut bound.common, 108, metadata.volume)?;
  bound.common[76..80].copy_from_slice(&metadata.attributes[..4]);
  bound.common[92..94].copy_from_slice(&metadata.attributes[4..]);
  bound.common[60..64].copy_from_slice(&metadata.unknown_type.to_le_bytes());
  if let Some(primitive) = &metadata.primitive {
    write_f32(&mut bound.common, 20, primitive.sphere_radius)?;
    write_vec3(&mut bound.common, 32, primitive.maximum)?;
    write_vec3(&mut bound.common, 48, primitive.minimum)?;
    write_vec3(&mut bound.common, 64, primitive.box_center)?;
    write_vec3(&mut bound.common, 80, primitive.sphere_center)?;
    bound.extension.clone_from(&primitive.extension);
  }
  match (&mut bound.geometry, metadata.geometry_unknowns) {
    (Some(geometry), Some([unknown_9c, unknown_ac])) => {
      geometry.unknown_9c = unknown_9c;
      geometry.unknown_ac = unknown_ac;
    }
    (None, None) => {}
    _ => return Err(invalid_input("YBN Geometry kind changed")),
  }
  Ok(())
}

fn vertex_index(
  geometry: &mut Geometry,
  position: [f32; 3],
  colour: Option<[u8; 4]>,
) -> io::Result<u16> {
  if geometry.vertex_colours.is_empty() != colour.is_none() {
    return Err(invalid_input("YBN vertex-colour layout cannot represent this polygon"));
  }
  if !geometry.vertex_colours.is_empty() && geometry.vertex_colours.len() != geometry.vertices.len()
  {
    return Err(invalid_data("YBN vertex-colour table does not match vertices"));
  }
  let local = std::array::from_fn(|axis| position[axis] - geometry.center[axis]);
  if local.iter().any(|value| !value.is_finite()) {
    return Err(invalid_data("Non-finite YBN polygon position"));
  }
  if let Some(index) = geometry.vertices.iter().enumerate().find_map(|(index, value)| {
    (value == &local && (colour.is_none() || geometry.vertex_colours[index] == colour.unwrap()))
      .then_some(index)
  }) {
    return u16::try_from(index).map_err(|_| invalid_data("YBN vertex index exceeds u16"));
  }
  let index = u16::try_from(geometry.vertices.len())
    .map_err(|_| invalid_data("YBN vertex table exceeds u16"))?;
  geometry.vertices.push(local);
  if let Some(colour) = colour {
    geometry.vertex_colours.push(colour);
  }
  Ok(index)
}

fn append_polygon(
  geometry: &mut Geometry,
  polygon: &ResolvedPolygon,
) -> io::Result<Polygon> {
  let mut material = polygon.material;
  if let Some(colour) = polygon.material_colour {
    let index =
      geometry.material_colours.iter().position(|existing| *existing == colour).unwrap_or_else(
        || {
          geometry.material_colours.push(colour);
          geometry.material_colours.len() - 1
        },
      );
    material.colour_index =
      u8::try_from(index).map_err(|_| invalid_data("YBN material colour index exceeds u8"))?;
  }
  let material_index =
    geometry.materials.iter().position(|existing| *existing == material).unwrap_or_else(|| {
      geometry.materials.push(material);
      geometry.materials.len() - 1
    });
  u8::try_from(material_index).map_err(|_| invalid_data("YBN material index exceeds u8"))?;
  let colour_at = |index: usize| polygon.vertex_colours.get(index).copied();
  let shape = &polygon.shape;
  Ok(match shape {
    ResolvedShape::Triangle {
      vertices,
      vertex_flags,
    } => Polygon::Triangle(Triangle {
      material: material_index as u8,
      vertices: [
        vertex_index(geometry, vertices[0], colour_at(0))?,
        vertex_index(geometry, vertices[1], colour_at(1))?,
        vertex_index(geometry, vertices[2], colour_at(2))?,
      ],
      vertex_flags: *vertex_flags,
      ..Triangle::default()
    }),
    ResolvedShape::Box {
      vertices,
    } => Polygon::Box {
      material: material_index as u8,
      vertices: [
        vertex_index(geometry, vertices[0], colour_at(0))?,
        vertex_index(geometry, vertices[1], colour_at(1))?,
        vertex_index(geometry, vertices[2], colour_at(2))?,
        vertex_index(geometry, vertices[3], colour_at(3))?,
      ],
    },
    ResolvedShape::Sphere {
      center,
      radius,
    } => Polygon::Sphere {
      material: material_index as u8,
      vertex: vertex_index(geometry, *center, colour_at(0))?,
      radius: *radius,
    },
    ResolvedShape::Capsule {
      endpoints,
      radius,
    } => Polygon::Capsule {
      material: material_index as u8,
      vertex1: vertex_index(geometry, endpoints[0], colour_at(0))?,
      vertex2: vertex_index(geometry, endpoints[1], colour_at(1))?,
      radius: *radius,
    },
    ResolvedShape::Cylinder {
      endpoints,
      radius,
    } => Polygon::Cylinder {
      material: material_index as u8,
      vertex1: vertex_index(geometry, endpoints[0], colour_at(0))?,
      vertex2: vertex_index(geometry, endpoints[1], colour_at(1))?,
      radius: *radius,
    },
  })
}
