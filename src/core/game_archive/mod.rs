//! Shared extraction and versioned publication for game archives.

pub(crate) mod archives;
mod build;
mod check;
mod hash_index;
mod manifest;
pub(crate) mod publication;
pub(crate) mod stage;

#[cfg(test)]
pub(crate) use crate::common::json::write_json;
pub(crate) use archives::{FileClassification, FileSelection};
#[cfg(test)]
pub(crate) use archives::{extract, sub_pack_count};
pub(crate) use build::BuildGameArchive;
pub(crate) use check::check;
pub(crate) use hash_index::{lod_name_candidates, write_hash_index};
pub use manifest::{CacheVersion, CachedFile, FileChange, VanillaCacheManifest};
pub(crate) use manifest::{load_manifest, load_manifest_with_revision};
#[cfg(test)]
pub(crate) use publication::Staging;
pub(crate) use publication::latest_version_marker;
#[cfg(test)]
pub(crate) use stage::stage;

pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[cfg(test)]
mod tests;
