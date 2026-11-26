//! Collision differences: YBNs compared as the polygons of the whole file grouped by collision
//! flags ([`polygons`]), and the merge in use ([`merge_ybn_deltas`]).

pub(super) mod legacy;
pub use legacy::{merge_ybn_deltas, merge_ybn_diffs};
mod merge_geometry;
mod polygon;
pub mod polygons;
mod resolve;

pub use polygon::{ResolvedPolygon, ResolvedShape};

#[cfg(test)]
mod tests;
