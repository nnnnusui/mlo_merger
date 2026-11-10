//! Read-only searches of the merged files and, with `--all`, the inputs the merge recorded.

mod entity;
mod file_from_position;
mod inputs;
mod occlude_position;
mod query;
pub(crate) mod rtree;
#[cfg(test)]
mod tests;
pub(crate) mod ybn;

use inputs::{Input, MergeCache, filename_filter, read_input, read_ymap, search_inputs};
use query::aabb_intersects_radius;
use std::path::PathBuf;

pub use inputs::InputStores;
pub use query::EntityQuery;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Searches existing merged binaries for entities, collision shapes or YMAP occluders.
#[derive(Debug, Clone)]
pub struct FindEntity {
  /// Validated GUID or three-dimensional position condition.
  pub query: EntityQuery,
  /// Directory containing merged YMAPs and YBNs.
  pub merged_dir: PathBuf,
  /// Where the inputs `--all` reads are.
  pub stores: InputStores,
  /// Optional case-insensitive filename glob.
  pub filter: Option<String>,
  /// Include recorded vanilla and source inputs.
  pub diff_all: bool,
}

/// Searches existing merged binaries for an entity GUID and renders matching data as XML.
#[derive(Debug, Clone)]
pub struct FindEntityGuid {
  /// Entity GUID, including the unsigned range above i32::MAX.
  pub guid: u32,
  /// Directory containing grouped or legacy flat merged YMAPs.
  pub merged_dir: PathBuf,
  /// Optional case-insensitive filename glob filter.
  pub filter: Option<String>,
  /// Include recorded vanilla and source inputs without regenerating artifacts.
  pub diff_all: bool,
}

impl FindEntityGuid {
  /// Returns one XML document containing every occurrence, including duplicate GUID entries.
  ///
  /// ```no_run
  /// let xml = mlo_merger::core::find::FindEntityGuid {
  ///   guid: 2443198849, merged_dir: "asset/merged".into(),
  ///   filter: Some("lr_cs4_10_strm_0.ymap".into()), diff_all: true,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<String> {
    FindEntity {
      query: EntityQuery::Guid(self.guid),
      merged_dir: self.merged_dir.clone(),
      stores: InputStores::default(),
      filter: self.filter.clone(),
      diff_all: self.diff_all,
    }
    .run()
  }
}

impl FindEntity {
  /// Returns a read-only XML search result using the same condition for all stages.
  ///
  /// ```no_run
  /// use mlo_merger::core::find::{EntityQuery, FindEntity};
  /// let xml = FindEntity {
  ///   query: EntityQuery::Position { position: [3.5, 4.2, 0.0], radius: 1.0 },
  ///   merged_dir: "asset/merged".into(), stores: Default::default(), filter: None,
  ///   diff_all: false,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<String> {
    self.query.validate()?;
    if let EntityQuery::FileFromPosition {
      position,
    } = &self.query
    {
      return file_from_position::run(self, *position);
    }
    if let EntityQuery::YbnPosition {
      position,
      radius,
      kind,
    } = &self.query
    {
      return ybn::run(self, *position, *radius, kind.as_deref());
    }
    if let EntityQuery::OccludePosition {
      position,
      radius,
    } = &self.query
    {
      return occlude_position::run(self, *position, *radius);
    }
    entity::run(self)
  }
}
