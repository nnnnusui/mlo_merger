//! Native YMAP decoding and encoding between typed models and META or PSO resources.

mod decode;
mod encode;
mod read;
mod schemas;
mod write;

pub use read::{ymap_entities_extents, ymap_to_model};
pub use write::{to_meta, write_ymap};

#[cfg(test)]
mod tests;
