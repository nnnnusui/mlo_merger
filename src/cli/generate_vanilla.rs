use std::path::PathBuf;

use bpaf::*;

use super::common::{self, CommonOptions};

/// Arguments for extracting versioned vanilla stream files from game archives.
#[derive(Debug, Clone)]
pub struct GenerateVanilla {
  /// Installed GTA V game directory.
  pub game_dir: PathBuf,
  /// Shared execution options.
  pub common: CommonOptions,
}

/// Parses `--generate-vanilla` arguments.
pub fn parser() -> impl Parser<GenerateVanilla> {
  let command = long("generate-vanilla")
    .help("Generate versioned vanilla stream files from game archives")
    .req_flag(());
  let game_dir = short('i').long("input").argument::<PathBuf>("GAME_DIR");
  let common = common::parser(PathBuf::from("asset/vanilla"));
  construct!(command, game_dir, common).map(|(_, game_dir, common)| GenerateVanilla {
    game_dir,
    common,
  })
}

/// Prints a mock invocation without reading game archives.
pub fn run_mock(command: GenerateVanilla) {
  println!("mock: --generate-vanilla game_dir={:?} options={:?}", command.game_dir, command.common);
}
