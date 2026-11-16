//! Ordered raw vanilla YMAP/YBN files from GTA V RPF archives.

use crate::platform::fs;
use std::{error::Error, path::PathBuf};

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

/// Generates versioned raw vanilla stream files from installed GTA V archives.
#[derive(Debug, Clone)]
pub struct GenerateVanilla {
  /// Installed GTA V Legacy directory.
  pub game_dir: PathBuf,
  /// Output directory for versioned raw vanilla files.
  pub output_dir: PathBuf,
  /// CodeWalker's `magic.dat`, needed only when generating.
  pub magic: PathBuf,
  /// Reuses a valid existing archive; otherwise generates only into an empty destination.
  pub reuse_existing: bool,
}

impl GenerateVanilla {
  /// Reuses an archive when configured, or reads the game archives to generate it.
  ///
  /// Reuse validates the existing archive without requiring an installed game or
  /// `magic.dat`. Generation extracts every stage.
  /// Logger initialization belongs to the calling application.
  ///
  /// ```no_run
  /// mlo_merger::core::vanilla::GenerateVanilla {
  ///   game_dir: "/mnt/gtav".into(), output_dir: "asset/vanilla".into(),
  ///   magic: "magic.dat".into(), reuse_existing: true,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> std::result::Result<(), Box<dyn Error>> {
    self.run_with_generation(|| self.generate())
  }

  fn run_with_generation(
    &self,
    generate: impl FnOnce() -> Result<()>,
  ) -> Result<()> {
    if self.reuse_existing {
      if fs::is_file(self.output_dir.join("cache_info.json")) {
        load_manifest(&self.output_dir)?;
        tracing::info!("Reusing existing vanilla archive");
        return Ok(());
      }
      if fs::is_dir(&self.output_dir)
        && crate::platform::fs::read_dir(&self.output_dir)?.next().is_some()
      {
        return Err(
          "Vanilla directory is nonempty but has no manifest; refusing automatic replacement"
            .into(),
        );
      }
      if !fs::is_dir(&self.game_dir) {
        return Err(format!("Vanilla archive is missing; specify --game-dir with an installed GTA V directory (current: {})", self.game_dir.display()).into());
      }
    }
    generate()
  }

  fn generate(&self) -> Result<()> {
    crate::core::game_archive::BuildGameArchive {
      game_dir: self.game_dir.clone(),
      output_dir: self.output_dir.clone(),
      magic: self.magic.clone(),
      files: FileSelection::MAPS,
    }
    .run_with_metadata(
      &["hash_names.json"],
      &["rpf_names.json"],
      |build, manifest, rpf_names| hash_index::write_hash_index(build, manifest, rpf_names),
    )
  }
}
