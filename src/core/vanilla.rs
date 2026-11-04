//! Ordered raw vanilla YMAP/YBN files from GTA V RPF archives.

use std::{error::Error, path::PathBuf};

mod archives;
mod generate;
mod hash_index;
mod io;
mod manifest;
mod publication;
mod stage;
mod versions;

pub(crate) use generate::BuildVanillaArchive;
pub use manifest::{CacheVersion, CachedFile, FileChange, VanillaCacheManifest};
pub use versions::{ListVanillaVersions, VanillaVersionChange, VanillaVersionEntry};

pub(crate) use io::{read_ymap, write_json};
pub(crate) use manifest::{load_manifest, load_manifest_with_revision};

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
      if self.output_dir.join("cache_info.json").is_file() {
        load_manifest(&self.output_dir)?;
        tracing::info!("Reusing existing vanilla archive");
        return Ok(());
      }
      if self.output_dir.is_dir() && std::fs::read_dir(&self.output_dir)?.next().is_some() {
        return Err(
          "Vanilla directory is nonempty but has no manifest; refusing automatic replacement"
            .into(),
        );
      }
      if !self.game_dir.is_dir() {
        return Err(format!("Vanilla archive is missing; specify --game-dir with an installed GTA V directory (current: {})", self.game_dir.display()).into());
      }
    }
    generate()
  }

  fn generate(&self) -> Result<()> {
    BuildVanillaArchive {
      game_dir: self.game_dir.clone(),
      output_dir: self.output_dir.clone(),
      magic: self.magic.clone(),
    }
    .run()
  }
}
