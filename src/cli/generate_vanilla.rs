use std::{
  collections::BTreeSet,
  path::{Path, PathBuf},
};

use bpaf::*;

use super::common::{self, CommonOptions};
use crate::core::game_archive::ArchiveContent;

/// Arguments for extracting versioned vanilla stream files from game archives.
#[derive(Debug, Clone)]
pub struct GenerateVanilla {
  /// Installed GTA V game directory.
  pub game_dir: PathBuf,
  /// CodeWalker's `magic.dat`.
  pub magic: PathBuf,
  /// FiveM's `game-storage` directory holding the game builds' title updates.
  pub game_storage: Option<PathBuf>,
  /// The content sets to extract.
  pub contents: BTreeSet<ArchiveContent>,
  /// Shared execution options.
  pub common: CommonOptions,
}

/// Parses `--generate-vanilla` arguments.
pub fn parser(asset: &Path) -> impl Parser<GenerateVanilla> + use<> {
  options(asset)
    .to_options()
    .descr("Generate versioned vanilla stream files from game archives")
    .command("--generate-vanilla")
}

fn options(asset: &Path) -> impl Parser<GenerateVanilla> + use<> {
  let game_dir = short('i').long("input").argument::<PathBuf>("GAME_DIR");
  let magic = magic_parser();
  let game_storage = game_storage_parser();
  let contents = long("content")
    .help("Content to extract: all, map or clothing; the existing archive's content is kept (default: all)")
    .argument::<String>("CONTENT")
    .parse(|value| parse_contents(&value))
    .fallback(ArchiveContent::ALL.into());
  let common = common::parser(asset, asset.join("vanilla"));
  construct!(game_dir, magic, game_storage, contents, common).map(
    |(game_dir, magic, game_storage, contents, common)| GenerateVanilla {
      game_dir,
      magic,
      game_storage,
      contents,
      common,
    },
  )
}

fn parse_contents(value: &str) -> Result<BTreeSet<ArchiveContent>, String> {
  if value == "all" {
    return Ok(ArchiveContent::ALL.into());
  }
  Ok([value.parse()?].into())
}

/// Parses the `magic.dat` path used to decrypt game archives.
pub fn magic_parser() -> impl Parser<PathBuf> {
  long("magic")
    .help("CodeWalker magic.dat for decrypting game archives (default: ./magic.dat)")
    .argument::<PathBuf>("FILE")
    .fallback(PathBuf::from("magic.dat"))
}

/// Parses FiveM's `game-storage` directory, from which each game build's title-update patches of
/// the DLC packs are read.
pub fn game_storage_parser() -> impl Parser<Option<PathBuf>> {
  long("game-storage")
    .help("FiveM's game-storage directory (FiveM.app/data/game-storage), whose copies of each game build's update.rpf give the build's DLC patches; without it, the installed game's are used for every build")
    .argument::<PathBuf>("DIR")
    .optional()
}

/// Generates the raw vanilla archive.
pub fn run(command: GenerateVanilla) -> Result<String, Box<dyn std::error::Error>> {
  crate::app::vanilla::GenerateVanilla {
    game_dir: command.game_dir,
    output_dir: command.common.output,
    magic: command.magic,
    game_storage: command.game_storage,
    contents: command.contents,
    reuse_existing: false,
  }
  .run()?;
  Ok(String::new())
}
