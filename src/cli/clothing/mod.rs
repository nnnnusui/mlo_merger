//! CLI operations for collecting clothing data, under the `--clothing` command.

pub mod generate_vanilla_cache;

use std::path::Path;

use bpaf::*;

/// A parsed clothing operation (`--clothing`).
#[derive(Debug, Clone)]
pub enum Clothing {
  /// Count clothing definitions per vanilla stage.
  GenerateVanillaCache(generate_vanilla_cache::GenerateVanillaClothingCache),
}

/// Parses `--clothing` and its operation.
pub fn parser(asset: &Path) -> impl Parser<Clothing> + use<> {
  let generate_vanilla_cache =
    generate_vanilla_cache::parser(asset).map(Clothing::GenerateVanillaCache);
  construct!([generate_vanilla_cache])
    .to_options()
    .descr("Count vanilla and DLC clothing data")
    .command("--clothing")
}

/// Runs the selected clothing operation.
pub fn run(command: Clothing) -> Result<String, Box<dyn std::error::Error>> {
  match command {
    Clothing::GenerateVanillaCache(command) => generate_vanilla_cache::run(command),
  }
}
