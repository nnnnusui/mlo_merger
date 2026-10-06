//! Infers per-resource vanilla stages and writes MLO-relative YMAP diff caches.

mod comparison;
mod generate;
mod io;
mod metadata;
mod resource;
mod types;
mod vanilla;

pub(crate) use comparison::ymap_distance;
pub use generate::BuildDiffCache;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type ModelReader = fn(&std::path::Path) -> Result<crate::core::format::ymap::model::Ymap>;

#[cfg(test)]
mod tests;
