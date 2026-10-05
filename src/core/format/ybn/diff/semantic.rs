use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::io;

use super::{
  compare,
  polygon::PolygonDiff,
  resolve::{self, BoundMetadata},
};
use crate::core::format::ybn::model::Bound;

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
  /// Extracts changes using the existing 5 mm collision matching policy.
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
    Self::with_tolerance(before, after, 0.005)
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
}
