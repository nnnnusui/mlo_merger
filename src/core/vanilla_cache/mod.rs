//! Ordered vanilla YMAP/YBN files and exact history deltas from GTA V RPF archives.

mod archives;
mod generate;
mod io;
mod logging;
mod manifest;
mod publication;
mod stage;
mod versions;
pub(crate) mod ybn_delta;
pub(crate) mod ymap_delta;

pub use generate::BuildVanillaCache;
pub use logging::version_logger;
pub use manifest::{CacheVersion, CachedFile, FileChange, VanillaCacheManifest};
pub use versions::{ListVanillaVersions, VanillaVersionChange, VanillaVersionEntry};

pub(crate) use archives::game_path;
pub(crate) use io::{read_ymap, write_json};
pub(crate) use logging::VersionLog;
pub(crate) use manifest::load_manifest;

#[cfg(test)]
pub(crate) use logging::init_test_version_logger;
#[cfg(test)]
mod tests;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
