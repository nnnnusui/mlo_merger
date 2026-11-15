use std::path::{Path, PathBuf};

use bpaf::*;

use crate::cli::{
  common::{self, CommonOptions},
  generate_vanilla::magic_parser,
};

/// Arguments for collecting vanilla and DLC Ped clothing variation definitions.
#[derive(Debug, Clone)]
pub struct GenerateVanillaClothing {
  /// Installed GTA V game directory.
  pub game_dir: PathBuf,
  /// CodeWalker's `magic.dat`.
  pub magic: PathBuf,
  /// Shared execution options.
  pub common: CommonOptions,
}

/// Parses `--generate-vanilla-clothing` arguments.
pub fn parser(asset: &Path) -> impl Parser<GenerateVanillaClothing> + use<> {
  let game_dir = short('i').long("input").argument::<PathBuf>("GAME_DIR");
  let magic = magic_parser();
  let common = common::parser(asset, asset.join("vanilla-clothing"));
  construct!(game_dir, magic, common)
    .map(|(game_dir, magic, common)| GenerateVanillaClothing {
      game_dir,
      magic,
      common,
    })
    .to_options()
    .descr("Collect vanilla and DLC clothing and appearance variation YMTs for all Peds")
    .command("--generate-vanilla-clothing")
}

/// Collects the raw clothing archive through the core workflow.
pub fn run(command: GenerateVanillaClothing) -> Result<String, Box<dyn std::error::Error>> {
  crate::core::clothing::vanilla::GenerateVanillaClothing {
    game_dir: command.game_dir,
    output_dir: command.common.output,
    magic: command.magic,
  }
  .run()?;
  Ok(String::new())
}
