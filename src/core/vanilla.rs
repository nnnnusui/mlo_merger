//! Ordered raw vanilla YMAP/YBN files from GTA V RPF archives.

use std::{error::Error, path::PathBuf};

use simplelog::{ColorChoice, CombinedLogger, Config, LevelFilter, TermLogger, TerminalMode};

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
pub(crate) use logging::VersionLog;
pub(crate) use manifest::load_manifest;

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
}

impl GenerateVanilla {
  /// Loads CodeWalker.Core and generates the requested vanilla archive prefix.
  pub fn run(&self) -> std::result::Result<(), Box<dyn Error>> {
    CombinedLogger::init(vec![
      TermLogger::new(LevelFilter::Info, Config::default(), TerminalMode::Mixed, ColorChoice::Auto),
      version_logger(),
    ])?;

    self.run_with_initialized_logger()
  }

  /// Generates the archive when the caller already owns the shared pipeline logger.
  pub(crate) fn run_with_initialized_logger(&self) -> std::result::Result<(), Box<dyn Error>> {
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
