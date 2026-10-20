use bpaf::*;
use std::path::PathBuf;

use crate::core::find::rtree::GenerateRTree;

#[derive(Debug, Clone)]
pub struct GenerateRTreeJson {
  pub output_dir: PathBuf,
  pub vanilla_cache_dir: PathBuf,
  pub source_cache_dir: PathBuf,
  pub merged_dir: PathBuf,
  pub force: bool,
}

pub fn parser() -> impl Parser<GenerateRTreeJson> {
  let command = long("generate-rtree-json").req_flag(());
  let output_dir = short('o')
    .long("output")
    .help("Directory for stream_file_rtree-{vanilla,source,merged}.json")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/read-cache"));
  let vanilla_cache_dir = long("vanilla-cache")
    .help("Derived vanilla cache directory (default: asset/vanilla-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/vanilla-cache"));
  let source_cache_dir = long("source-cache")
    .help("Source cache directory (default: asset/source-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/source-cache"));
  let merged_dir = long("merged")
    .help("Merge output directory (default: asset/merged)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/merged"));
  let force = short('f')
    .long("force")
    .help("Rebuild every index, even one whose store is unchanged")
    .switch();
  construct!(command, output_dir, vanilla_cache_dir, source_cache_dir, merged_dir, force).map(
    |(_, output_dir, vanilla_cache_dir, source_cache_dir, merged_dir, force)| GenerateRTreeJson {
      output_dir,
      vanilla_cache_dir,
      source_cache_dir,
      merged_dir,
      force,
    },
  )
}

pub fn run(command: GenerateRTreeJson) -> Result<(), Box<dyn std::error::Error>> {
  let outcomes = GenerateRTree {
    vanilla_cache_dir: command.vanilla_cache_dir,
    source_cache_dir: command.source_cache_dir,
    merged_dir: command.merged_dir,
    output_dir: command.output_dir,
    force: command.force,
  }
  .run()?;
  for outcome in outcomes {
    let state = if outcome.updated { "Wrote" } else { "Kept current" };
    println!(
      "{state} {} stream-file R-tree JSON at {}",
      outcome.store.name(),
      outcome.output.display()
    );
  }
  Ok(())
}
