use std::path::{Path, PathBuf};

use bpaf::*;

use crate::cli::common::clothing_dir;

/// Arguments for counting clothing definitions per vanilla stage.
#[derive(Debug, Clone)]
pub struct GenerateVanillaClothingCache {
  /// Vanilla archive directory.
  pub vanilla_dir: PathBuf,
  /// Output directory for the counts.
  pub output: PathBuf,
  /// Rebuilds even when the cache is current.
  pub force: bool,
}

/// Parses `--clothing --generate-vanilla-cache` arguments.
pub fn parser(asset: &Path) -> impl Parser<GenerateVanillaClothingCache> + use<> {
  let vanilla_dir = short('i')
    .long("input")
    .help("Vanilla archive holding clothing content (default: asset/vanilla)")
    .argument::<PathBuf>("DIR")
    .fallback(asset.join("vanilla"));
  let output = short('o')
    .long("output")
    .help("Output directory (default: asset/clothing/vanilla-cache)")
    .argument::<PathBuf>("PATH")
    .fallback(clothing_dir(asset).join("vanilla-cache"));
  let force = short('f').long("force").help("Force regeneration of this operation").switch();
  construct!(vanilla_dir, output, force)
    .map(|(vanilla_dir, output, force)| GenerateVanillaClothingCache {
      vanilla_dir,
      output,
      force,
    })
    .to_options()
    .descr(
      "Count clothing definitions per vanilla stage from the vanilla archive's clothing content",
    )
    .command("--generate-vanilla-cache")
}

/// Counts the clothing archive through the application workflow.
pub fn run(command: GenerateVanillaClothingCache) -> Result<String, Box<dyn std::error::Error>> {
  crate::app::clothing::BuildClothingCache {
    vanilla_dir: command.vanilla_dir,
    output_dir: command.output,
    force: command.force,
  }
  .run()?;
  Ok(String::new())
}
