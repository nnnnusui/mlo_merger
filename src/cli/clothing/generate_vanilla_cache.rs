use std::path::{Path, PathBuf};

use bpaf::*;

/// Arguments for counting clothing definitions per vanilla stage.
#[derive(Debug, Clone)]
pub struct GenerateVanillaClothingCache {
  /// Clothing archive directory.
  pub clothing_dir: PathBuf,
  /// Output directory for the counts.
  pub output: PathBuf,
  /// Rebuilds even when the cache is current.
  pub force: bool,
}

/// Parses `--generate-vanilla-clothing-cache` arguments.
pub fn parser(asset: &Path) -> impl Parser<GenerateVanillaClothingCache> + use<> {
  let clothing_dir = long("clothing")
    .help("Clothing archive directory (default: asset/vanilla-clothing)")
    .argument::<PathBuf>("DIR")
    .fallback(asset.join("vanilla-clothing"));
  let output = short('o')
    .long("output")
    .help("Output directory")
    .argument::<PathBuf>("PATH")
    .fallback(asset.join("vanilla-clothing-cache"));
  let force = short('f').long("force").help("Force regeneration of this operation").switch();
  construct!(clothing_dir, output, force)
    .map(|(clothing_dir, output, force)| GenerateVanillaClothingCache {
      clothing_dir,
      output,
      force,
    })
    .to_options()
    .descr("Count clothing definitions per vanilla stage from the clothing archive")
    .command("--generate-vanilla-clothing-cache")
}

/// Counts the clothing archive through the core workflow.
pub fn run(command: GenerateVanillaClothingCache) -> Result<String, Box<dyn std::error::Error>> {
  crate::core::clothing::cache::BuildClothingCache {
    clothing_dir: command.clothing_dir,
    output_dir: command.output,
    force: command.force,
  }
  .run()?;
  Ok(String::new())
}
