use std::{
  collections::HashMap,
  path::{Path, PathBuf},
};

use crate::core::{
  format::ymap::binary::ymap_to_model,
  game_archive::{BuildGameArchive, FileSelection, load_manifest, write_hash_index},
};
use crate::platform::fs;

#[cfg(test)]
mod tests;

/// Generates or reuses the versioned raw vanilla map archive.
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
  /// Reuses a valid archive when configured, or generates one from the game archives.
  ///
  /// Reuse validates the existing archive without requiring an installed game or `magic.dat`.
  /// Logger initialization belongs to the calling application.
  ///
  /// ```no_run
  /// mlo_merger::app::map::GenerateVanilla {
  ///   game_dir: "/mnt/gtav".into(), output_dir: "asset/vanilla".into(),
  ///   magic: "magic.dat".into(), reuse_existing: true,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
    generate_archive(&self.game_dir, &self.output_dir, &self.magic, self.reuse_existing)
  }
}

pub(crate) fn generate_archive(
  game_dir: &Path,
  output_dir: &Path,
  magic: &Path,
  reuse_existing: bool,
) -> Result<(), Box<dyn std::error::Error>> {
  run_with_generation(game_dir, output_dir, reuse_existing, || {
    BuildGameArchive {
      game_dir: game_dir.to_path_buf(),
      output_dir: output_dir.to_path_buf(),
      magic: magic.to_path_buf(),
      files: FileSelection::MAPS,
    }
    .run_with_metadata(&["hash_names.json"], &["rpf_names.json"], write_hash_index)
  })
}

/// Reuses a valid archive when requested, and only generates into an empty destination.
fn run_with_generation(
  game_dir: &Path,
  output_dir: &Path,
  reuse_existing: bool,
  generate: impl FnOnce() -> std::result::Result<(), Box<dyn std::error::Error>>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
  if reuse_existing {
    if fs::is_file(output_dir.join("cache_info.json")) {
      load_manifest(output_dir)?;
      tracing::info!("Reusing existing vanilla archive");
      return Ok(());
    }
    if fs::is_dir(output_dir) && fs::read_dir(output_dir)?.next().is_some() {
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

/// Reads a native YMAP file into the map model.
pub(crate) fn read_ymap(
  path: &Path
) -> Result<crate::core::format::ymap::model::ymap::Ymap, Box<dyn std::error::Error>> {
  Ok(ymap_to_model(&fs::read(path)?, &HashMap::new())?)
}
