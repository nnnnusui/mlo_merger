//! Extraction of the raw vanilla archive every feature reads.

use std::{
  collections::BTreeSet,
  path::{Path, PathBuf},
};

use crate::core::game_archive::{
  ArchiveContent, BuildGameArchive, FileSelection, load_manifest, missing_content, write_hash_index,
};
use crate::platform::fs;

#[cfg(test)]
mod tests;

/// Generates or reuses the versioned raw vanilla archive.
#[derive(Debug, Clone)]
pub struct GenerateVanilla {
  /// Installed GTA V Legacy directory.
  pub game_dir: PathBuf,
  /// Output directory for versioned raw vanilla files.
  pub output_dir: PathBuf,
  /// CodeWalker's `magic.dat`, needed only when generating.
  pub magic: PathBuf,
  /// The content sets to extract. Generation also extracts those the existing archive holds.
  pub contents: BTreeSet<ArchiveContent>,
  /// Reuses a valid existing archive holding `contents`; otherwise generates only into an
  /// empty destination.
  pub reuse_existing: bool,
}

impl GenerateVanilla {
  /// Reuses a valid archive when configured, or generates one from the game archives.
  ///
  /// Reuse validates the existing archive without requiring an installed game or `magic.dat`.
  /// Logger initialization belongs to the calling application.
  ///
  /// ```no_run
  /// use mlo_merger::core::game_archive::ArchiveContent;
  /// mlo_merger::app::vanilla::GenerateVanilla {
  ///   game_dir: "/mnt/gtav".into(), output_dir: "asset/vanilla".into(),
  ///   magic: "magic.dat".into(), contents: ArchiveContent::ALL.into(), reuse_existing: true,
  /// }.run()?;
  /// # Ok::<(), Box<dyn std::error::Error>>(())
  /// ```
  pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
    run_with_generation(
      &self.game_dir,
      &self.output_dir,
      &self.contents,
      self.reuse_existing,
      |contents| {
        BuildGameArchive {
          game_dir: self.game_dir.clone(),
          output_dir: self.output_dir.clone(),
          magic: self.magic.clone(),
          files: FileSelection::new(contents),
        }
        .run_with_metadata(&["hash_names.json"], &["rpf_names.json"], write_hash_index)
      },
    )
  }
}

/// Reuses a valid archive holding `contents` when requested, and only generates into an empty
/// destination then; `generate` receives the content sets to extract.
fn run_with_generation(
  game_dir: &Path,
  output_dir: &Path,
  contents: &BTreeSet<ArchiveContent>,
  reuse_existing: bool,
  generate: impl FnOnce(BTreeSet<ArchiveContent>) -> Result<(), Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
  if contents.is_empty() {
    return Err("No vanilla archive content selected".into());
  }
  let existing = fs::is_file(crate::core::game_archive::manifest_path(output_dir));
  if reuse_existing {
    if existing {
      let manifest = load_manifest(output_dir)?;
      if let Some(&content) = contents.iter().find(|content| !manifest.contents.contains(content)) {
        return Err(missing_content(content));
      }
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
  let mut extracted = contents.clone();
  if existing && let Ok(manifest) = load_manifest(output_dir) {
    extracted.extend(manifest.contents.iter().copied());
  }
  if extracted != *contents {
    tracing::info!(
      "Also extracting the content the existing archive holds: {}",
      extracted.difference(contents).map(|content| content.name()).collect::<Vec<_>>().join(", ")
    );
  }
  generate(extracted)
}
