//! Ordered vanilla YMAP snapshots and exact parsed-state deltas from GTA V RPF archives.

mod archives;
mod generate;
mod io;
mod logging;
mod manifest;
mod publication;
mod stage;
pub(crate) mod ymap_delta;

pub use generate::BuildGtavCache;
pub use logging::version_logger;
pub use manifest::{CacheVersion, CachedFile, FileChange, GtavCacheManifest};

pub(crate) use archives::game_path;
pub(crate) use io::{read_ymap, write_json};
pub(crate) use logging::VersionLog;

#[cfg(test)]
pub(crate) use logging::init_test_version_logger;
#[cfg(test)]
mod tests;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
