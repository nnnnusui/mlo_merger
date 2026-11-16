use std::path::PathBuf;

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
    crate::core::clothing::vanilla::generate_archive(&self.game_dir, &self.output_dir, &self.magic)
  }
}
