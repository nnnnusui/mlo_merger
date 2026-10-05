//! Collision differences and existing vanilla-relative merge behavior.

pub(super) mod legacy;
pub use legacy::merge_ybn_deltas;
mod compare;
mod polygon;
mod resolve;
mod semantic;

pub use polygon::{PolygonDiff, PolygonLocation, ResolvedPolygon, ResolvedShape};
pub use resolve::{BoundMetadata, PrimitiveBounds};
pub use semantic::{BoundDiff, YbnDiff};

#[cfg(test)]
mod tests;
