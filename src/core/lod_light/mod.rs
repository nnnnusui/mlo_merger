//! Resource-local LOD light snapshots and validated removal edits.

mod edits;
mod export;
mod geometry;
mod io;
mod merge;
mod metadata;
mod overwrite_cache;
mod resource;
mod snapshot;
mod types;

pub use export::{ExportLightResult, ExportLights};
pub use types::*;

pub(crate) use merge::PreparedEdits;
pub(crate) use resource::inventory;
pub(crate) use snapshot::{LoadedMap, read_map};

#[cfg(test)]
mod tests;

pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
