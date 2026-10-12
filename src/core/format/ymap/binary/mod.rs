//! Native YMAP decoding and encoding from typed models and compatible META schemas.

mod light_validation;
mod meta;
mod model;
mod read;
mod values;
mod write;

pub(crate) use read::{
  ymap_to_light_model_with_entities, ymap_to_model_with_entities,
  ymap_to_model_with_entities_from_meta,
};
pub use read::{ymap_to_model, ymap_to_xml};
pub use write::{to_meta, write_ymap};

#[cfg(test)]
mod tests;
