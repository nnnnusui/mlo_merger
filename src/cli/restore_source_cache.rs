use std::path::PathBuf;

use bpaf::*;

/// Arguments for returning cached stream files to source resources.
#[derive(Debug, Clone)]
pub struct RestoreSourceCache {
  /// Destination source resource root.
  pub source_dir: PathBuf,
  /// Source cache holding the recorded original paths.
  pub cache_dir: PathBuf,
  /// Resource names or source-relative resource paths to restore; none restores every resource.
  pub resources: Vec<String>,
}

/// Parses `--restore-source-cache [RESOURCE...]` arguments.
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
  let resources = positional::<String>("RESOURCE")
    .help("Restore only these resources (names or source-relative paths)")
    .many();
  construct!(command, source_dir, cache_dir, yes, resources).map(
    |(_, source_dir, cache_dir, _, resources)| RestoreSourceCache {
      source_dir,
      cache_dir,
      resources,
    },
  )
}

/// Restores source files without generating caches, merging or deploying.
pub fn run(command: RestoreSourceCache) -> Result<(), Box<dyn std::error::Error>> {
  let restored = crate::core::source_cache::restore_source_cache(
    &command.cache_dir,
    &command.source_dir,
    &command.resources,
  )?;
  println!("Restored {restored} source files.");
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn restores_every_resource_or_the_named_ones() {
    let parse = |args: &[&str]| parser().to_options().run_inner(args).unwrap();
    assert!(parse(&["--restore-source-cache"]).resources.is_empty());
    let restore = parse(&["--restore-source-cache", "-y", "alpha", "[maps]/beta"]);
    assert_eq!(restore.resources, ["alpha", "[maps]/beta"]);
    assert_eq!(restore.source_dir, PathBuf::from("asset/source"));
  }
}
