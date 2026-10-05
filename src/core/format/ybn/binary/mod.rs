//! Native resource decoding, encoding and regenerated acceleration data.

use super::*;
mod bvh;
mod helpers;
mod read;
mod write;
pub(in crate::core::format::ybn) use helpers::*;
pub(in crate::core::format::ybn) use read::read_bound;
pub use read::read_ybn_root;
pub(in crate::core::format::ybn) use write::{compact_polygon_materials, encode_ybn_bound};
