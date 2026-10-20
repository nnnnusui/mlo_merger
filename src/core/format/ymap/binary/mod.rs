//! Native YMAP decoding and encoding from typed models and compatible META schemas.

mod decode;
mod meta;
mod model;
mod read;
mod values;
mod write;

pub use read::{ymap_entities_extents, ymap_to_model};
pub use write::{to_meta, write_ymap};

#[cfg(test)]
mod tests;
