//! Indexed collision geometry used by codecs and semantic comparisons.

mod bound;
mod geometry;
mod material;
mod polygon;

pub use bound::Bound;
pub use geometry::Geometry;
pub use material::Material;
pub use polygon::{Polygon, Triangle};
