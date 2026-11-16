//! Ordered raw vanilla YMAP/YBN files from GTA V RPF archives.

use std::path::Path;

use crate::platform::fs;

mod check;
mod hash_index;
mod io;
mod versions;

pub(crate) use crate::core::game_archive::FileSelection;
pub use crate::core::game_archive::{CacheVersion, CachedFile, FileChange, VanillaCacheManifest};
#[cfg(test)]
pub(crate) use crate::core::game_archive::{Staging, extract, stage, sub_pack_count};
pub(crate) use check::check;
pub use versions::{ListVanillaVersions, VanillaVersionChange, VanillaVersionEntry};

pub(crate) use crate::core::game_archive::{load_manifest, load_manifest_with_revision};
pub(crate) use hash_index::write_hash_index;
pub(crate) use io::{read_ymap, write_json};

#[cfg(test)]
mod tests;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Compatibility entry point; use [`crate::app::map::GenerateVanilla`] for new callers.
#[deprecated(note = "Use mlo_merger::app::map::GenerateVanilla")]
#[derive(Debug, Clone)]
pub struct GenerateVanilla {
  /// Installed GTA V Legacy directory.
  pub game_dir: std::path::PathBuf,
  /// Output directory for versioned raw vanilla files.
  pub output_dir: std::path::PathBuf,
  /// CodeWalker's `magic.dat`, needed only when generating.
  pub magic: std::path::PathBuf,
  /// Reuses a valid existing archive; otherwise generates only into an empty destination.
  pub reuse_existing: bool,
}

#[allow(deprecated)]
impl GenerateVanilla {
  /// Reuses a valid archive when configured, or generates one from the game archives.
  pub fn run(&self) -> Result<()> {
    generate_archive(&self.game_dir, &self.output_dir, &self.magic, self.reuse_existing)
  }
}

/// Runs the map archive generation policy shared with the core pipeline.
pub(crate) fn generate_archive(
  game_dir: &Path,
  output_dir: &Path,
  magic: &Path,
  reuse_existing: bool,
) -> Result<()> {
  run_with_generation(game_dir, output_dir, reuse_existing, || {
    crate::core::game_archive::BuildGameArchive {
      game_dir: game_dir.to_path_buf(),
      output_dir: output_dir.to_path_buf(),
      magic: magic.to_path_buf(),
      files: FileSelection::MAPS,
    }
    .run_with_metadata(
      &["hash_names.json"],
      &["rpf_names.json"],
      |build, manifest, rpf_names| hash_index::write_hash_index(build, manifest, rpf_names),
    )
  })
}

fn run_with_generation(
  game_dir: &Path,
  output_dir: &Path,
  reuse_existing: bool,
  generate: impl FnOnce() -> Result<()>,
) -> Result<()> {
  if reuse_existing {
    if fs::is_file(output_dir.join("cache_info.json")) {
      load_manifest(output_dir)?;
      tracing::info!("Reusing existing vanilla archive");
      return Ok(());
    }
    if fs::is_dir(output_dir) && crate::platform::fs::read_dir(output_dir)?.next().is_some() {
      return Err(
        "Vanilla directory is nonempty but has no manifest; refusing automatic replacement".into(),
      );
    }
    if !fs::is_dir(game_dir) {
      return Err(format!("Vanilla archive is missing; specify --game-dir with an installed GTA V directory (current: {})", game_dir.display()).into());
    }
  }
  generate()
}
