use std::path::{Path, PathBuf};

use crate::core::{
  clothing::vanilla::classify_ymt,
  game_archive::{BuildGameArchive, FileSelection},
  vanilla::write_hash_index,
};

#[cfg(test)]
mod tests;

pub(crate) const FILES: FileSelection = FileSelection {
  extensions: &["ymt"],
  classify: classify_ymt,
};

/// Collects Ped clothing, appearance variation, and creature metadata YMTs.
#[derive(Debug, Clone)]
pub struct GenerateVanillaClothing {
  /// Installed GTA V Legacy directory.
  pub game_dir: PathBuf,
  /// Output archive, normally `asset/vanilla-clothing`.
  pub output_dir: PathBuf,
  /// CodeWalker's `magic.dat`, used with `GTA5.exe` to decrypt the archives.
  pub magic: PathBuf,
}

impl GenerateVanillaClothing {
  /// Extracts raw supported clothing YMTs in vanilla stage order.
  ///
  /// Same-name overlays, unchanged-file reuse and output replacement follow map
  /// generation. This collects definitions, not drawable or texture assets.
  ///
  /// ```no_run
  /// mlo_merger::app::clothing::GenerateVanillaClothing {
  ///   game_dir: "/mnt/gtav".into(), output_dir: "asset/vanilla-clothing".into(),
  ///   magic: "magic.dat".into(),
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
    generate_archive(&self.game_dir, &self.output_dir, &self.magic)
  }
}

pub(crate) fn generate_archive(
  game_dir: &Path,
  output_dir: &Path,
  magic: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
  BuildGameArchive {
    game_dir: game_dir.to_path_buf(),
    output_dir: output_dir.to_path_buf(),
    magic: magic.to_path_buf(),
    files: FILES,
  }
  .run_with_metadata(&["hash_names.json"], &["rpf_names.json"], write_hash_index)
}
