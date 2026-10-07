//! Native YMAP encoding from typed models and compatible META schemas.

mod meta;
mod model;
mod values;
mod write;

pub use write::{to_meta, write_ymap};

#[cfg(test)]
mod tests;
