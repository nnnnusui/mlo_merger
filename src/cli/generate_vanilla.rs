use std::path::{Path, PathBuf};

use bpaf::*;

use super::common::{self, CommonOptions};

/// Arguments for extracting versioned vanilla stream files from game archives.
#[derive(Debug, Clone)]
pub struct GenerateVanilla {
  /// Installed GTA V game directory.
  pub game_dir: PathBuf,
  /// CodeWalker's `magic.dat`.
  pub magic: PathBuf,
  /// Shared execution options.
  pub common: CommonOptions,
}

/// Parses `--generate-vanilla` arguments.
pub fn parser(asset: &Path) -> impl Parser<GenerateVanilla> + use<> {
  let command = long("generate-vanilla")
    .help("Generate versioned vanilla stream files from game archives")
    .req_flag(());
  let game_dir = short('i').long("input").argument::<PathBuf>("GAME_DIR");
  let magic = magic_parser();
  let common = common::parser(asset, asset.join("vanilla"));
  construct!(command, game_dir, magic, common).map(|(_, game_dir, magic, common)| GenerateVanilla {
    game_dir,
    magic,
    common,
  })
}

/// Parses the `magic.dat` path used to decrypt game archives.
pub fn magic_parser() -> impl Parser<PathBuf> {
  long("magic")
    .help("CodeWalker magic.dat for decrypting game archives (default: ./magic.dat)")
    .argument::<PathBuf>("FILE")
    .fallback(PathBuf::from("magic.dat"))
}

/// Generates the raw vanilla archive through the core workflow.
pub fn run(command: GenerateVanilla) -> Result<String, Box<dyn std::error::Error>> {
  crate::core::vanilla::GenerateVanilla {
    game_dir: command.game_dir,
    output_dir: command.common.output,
    magic: command.magic,
    reuse_existing: false,
  }
  .run()?;
  Ok(String::new())
}
