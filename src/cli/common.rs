use std::path::{Path, PathBuf};

use bpaf::*;

use crate::core::config::gamebuild::{GameBuild, GameBuilds};

/// Options shared by the pipeline commands.
#[derive(Debug, Clone)]
pub struct CommonOptions {
  /// Output directory for the selected operation.
  pub output: PathBuf,
  /// Vanilla archive directory.
  pub vanilla: PathBuf,
  /// Derived vanilla cache directory.
  pub vanilla_cache: PathBuf,
  /// Derived source cache directory.
  pub source_cache: PathBuf,
  /// Optional upper bound for vanilla game versions.
  pub gamebuild: Option<String>,
  /// Forces regeneration of the selected operation's output.
  pub force: bool,
  /// Skips interactive confirmation.
  pub yes: bool,
  /// Explicit pipeline stages to run.
  pub step_names: Vec<String>,
}

/// Resolves `--gamebuild` against `[gamebuild]` in the config file.
pub(crate) fn gamebuild(
  requested: Option<&str>
) -> Result<Option<GameBuild>, Box<dyn std::error::Error>> {
  let Some(requested) = requested else {
    return Ok(None);
  };
  Ok(Some(GameBuilds::load(&crate::core::config::path())?.resolve(requested)?))
}

/// Parses paths and execution controls shared by CLI commands.
pub fn parser(
  asset: &Path,
  default_output: PathBuf,
) -> impl Parser<CommonOptions> + use<> {
  let output = short('o')
    .long("output")
    .help("Output directory")
    .argument::<PathBuf>("PATH")
    .fallback(default_output);
  let vanilla = long("vanilla")
    .help("Vanilla archive directory (default: asset/vanilla)")
    .argument::<PathBuf>("DIR")
    .fallback(asset.join("vanilla"));
  let vanilla_cache = long("vanilla-cache")
    .help("Derived vanilla cache directory (default: asset/vanilla-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(asset.join("vanilla-cache"));
  let source_cache = long("source-cache")
    .help("Derived source cache directory (default: asset/source-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(asset.join("source-cache"));
  let gamebuild = long("gamebuild")
    .help("Game build the vanilla cache targets: a build number or DLC name from [gamebuild] in the config file")
    .argument::<String>("BUILD")
    .optional();
  let force = short('f').long("force").help("Force regeneration of this operation").switch();
  let yes = short('y').long("yes").help("Skip confirmation prompts").switch();
  let step_names = long("step-name")
    .help("Select a pipeline stage; may be specified more than once")
    .argument::<String>("NAME")
    .many();

  construct!(output, vanilla, vanilla_cache, source_cache, gamebuild, force, yes, step_names).map(
    |(output, vanilla, vanilla_cache, source_cache, gamebuild, force, yes, step_names)| {
      CommonOptions {
        output,
        vanilla,
        vanilla_cache,
        source_cache,
        gamebuild,
        force,
        yes,
        step_names,
      }
    },
  )
}
