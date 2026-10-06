use std::path::PathBuf;

use bpaf::*;

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

/// Parses paths and execution controls shared by CLI commands.
pub fn parser(default_output: PathBuf) -> impl Parser<CommonOptions> {
  let output = short('o')
    .long("output")
    .help("Output directory")
    .argument::<PathBuf>("PATH")
    .fallback(default_output);
  let vanilla = long("vanilla")
    .help("Vanilla archive directory (default: asset/vanilla)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/vanilla"));
  let vanilla_cache = long("vanilla-cache")
    .help("Derived vanilla cache directory (default: asset/vanilla-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/vanilla-cache"));
  let source_cache = long("source-cache")
    .help("Derived source cache directory (default: asset/source-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/source-cache"));
  let gamebuild = long("gamebuild")
    .help("Latest vanilla build/version to include; later versions are ignored")
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

/// Prints a placeholder invocation until the pipeline implementation is connected.
pub fn run_mock(
  command_name: &str,
  options: &CommonOptions,
) {
  println!("mock: {command_name} {options:?}");
}
