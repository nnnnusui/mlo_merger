use std::path::PathBuf;

use bpaf::*;

/// Arguments for returning cached stream files to source resources.
#[derive(Debug, Clone)]
pub struct RestoreSourceCache {
  /// Destination source resource root.
  pub source_dir: PathBuf,
  /// Source cache holding the recorded original paths.
  pub cache_dir: PathBuf,
}

/// Parses `--restore-source-cache` arguments.
pub fn parser() -> impl Parser<RestoreSourceCache> {
  let command = long("restore-source-cache")
    .help(
      "Move active cached files back to their original source paths without overwriting changes",
    )
    .req_flag(());
  let source_dir = short('i')
    .long("input")
    .help("Source resource directory (default: asset/source)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/source"));
  let cache_dir = long("source-cache")
    .help("Source cache directory (default: asset/source-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/source-cache"));
  let yes = short('y').long("yes").help("Accepted for non-interactive execution").switch();
  construct!(command, source_dir, cache_dir, yes).map(|(_, source_dir, cache_dir, _)| {
    RestoreSourceCache {
      source_dir,
      cache_dir,
    }
  })
}

/// Restores source files without generating caches, merging or deploying.
pub fn run(command: RestoreSourceCache) -> Result<(), Box<dyn std::error::Error>> {
  let restored =
    crate::core::source_cache::restore_source_cache(&command.cache_dir, &command.source_dir)?;
  println!("Restored {restored} source files.");
  Ok(())
}
