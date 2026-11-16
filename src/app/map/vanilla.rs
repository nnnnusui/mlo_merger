use std::path::PathBuf;

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
    crate::core::vanilla::generate_archive(
      &self.game_dir,
      &self.output_dir,
      &self.magic,
      self.reuse_existing,
    )
  }
}
