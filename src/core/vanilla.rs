//! Ordered raw vanilla YMAP/YBN files from GTA V RPF archives.

use std::{error::Error, path::PathBuf};

mod archives;
mod generate;
mod hash_index;
mod io;
mod logging;
mod manifest;
mod publication;
mod stage;
mod versions;

pub(crate) use generate::BuildVanillaArchive;
pub use logging::version_logger;
pub use manifest::{CacheVersion, CachedFile, FileChange, VanillaCacheManifest};
pub use versions::{ListVanillaVersions, VanillaVersionChange, VanillaVersionEntry};

pub(crate) use io::{read_ymap, write_json};
pub(crate) use manifest::{load_manifest, load_manifest_with_revision};

#[cfg(test)]
pub(crate) use logging::init_test_version_logger;
#[cfg(test)]
mod tests;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

use super::codewalker::CodeWalker;

/// Generates versioned raw vanilla stream files from installed GTA V archives.
#[derive(Debug, Clone)]
pub struct GenerateVanilla {
  /// Installed GTA V Legacy directory.
  pub game_dir: PathBuf,
  /// Output directory for versioned raw vanilla files.
  pub output_dir: PathBuf,
  /// Optional final installed version/stage to include.
  pub gamebuild: Option<String>,
  /// Reuses a valid existing archive; otherwise generates only into an empty destination.
  pub reuse_existing: bool,
}

impl GenerateVanilla {
  /// Reuses an archive when configured, or loads CodeWalker to generate it.
  ///
  /// Reuse validates the existing archive without requiring an installed game or
  /// loading the bridge. `gamebuild` selects extraction stages only when generating.
  /// Logger initialization belongs to the calling application.
  ///
  /// ```no_run
  /// mlo_merger::core::vanilla::GenerateVanilla {
  ///   game_dir: "/mnt/gtav".into(), output_dir: "asset/vanilla".into(),
  ///   gamebuild: None, reuse_existing: true,
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
        log::info!("Reusing existing vanilla archive");
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
    let bridge_dll =
      std::env::var_os("CODEWALKER_BRIDGE_DLL").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from("bridge/CodeWalker.Bridge/bin/publish/CodeWalker.Bridge.dll")
      });
    let codewalker = CodeWalker::init(&bridge_dll).map_err(|error| {
      format!(
        "Failed to load CodeWalker.Bridge from {} ({error}). Build it with `dotnet publish bridge/CodeWalker.Bridge -c Release -o bridge/CodeWalker.Bridge/bin/publish -p:CodeWalkerCoreDllPath=<path to CodeWalker.Core.dll>`, or set CODEWALKER_BRIDGE_DLL.",
        bridge_dll.display()
      )
    })?;
    BuildVanillaArchive {
      game_dir: self.game_dir.clone(),
      output_dir: self.output_dir.clone(),
      through_version: self.gamebuild.clone(),
    }
    .run(&codewalker)
  }
}
