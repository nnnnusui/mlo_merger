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
  let contents = long("content")
    .help("Content to extract: all, map or clothing; the existing archive's content is kept (default: all)")
    .argument::<String>("CONTENT")
    .parse(|value| parse_contents(&value))
    .fallback(ArchiveContent::ALL.into());
  let common = common::parser(asset, asset.join("vanilla"));
  construct!(game_dir, magic, contents, common).map(|(game_dir, magic, contents, common)| {
    GenerateVanilla {
      game_dir,
      magic,
      contents,
      common,
    }
  })
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

/// Generates the raw vanilla archive.
pub fn run(command: GenerateVanilla) -> Result<String, Box<dyn std::error::Error>> {
  crate::app::vanilla::GenerateVanilla {
    game_dir: command.game_dir,
    output_dir: command.common.output,
    magic: command.magic,
    contents: command.contents,
    reuse_existing: false,
  }
  .run()?;
  Ok(String::new())
}
