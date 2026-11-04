//! Collision differences ([`YbnDiff`]) and the merge in use ([`merge_ybn_deltas`], in
//! `legacy`), which works on the files themselves until YBNs get diff documents.

pub(super) mod legacy;
pub use legacy::merge_ybn_deltas;
mod compare;
mod merge_geometry;
mod polygon;
mod resolve;
mod semantic;

pub use polygon::{PolygonDiff, PolygonLocation, ResolvedPolygon, ResolvedShape};
pub use resolve::{BoundMetadata, PrimitiveBounds};
pub use semantic::{BoundDiff, YbnDiff};

#[cfg(test)]
mod tests;
