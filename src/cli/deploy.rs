use super::common::{self, CommonOptions};
use bpaf::*;
use std::path::PathBuf;

/// Arguments for deploying generated merged and cached stream files.
#[derive(Debug, Clone)]
pub struct Deploy {
  /// Directory of generated merged files.
  pub merged_dir: PathBuf,
  /// Shared output, source-cache and force options.
  pub common: CommonOptions,
}

/// Parses `--deploy` arguments.
pub fn parser() -> impl Parser<Deploy> {
  let command =
    long("deploy").help("Copy merged and remaining cached files into a stream layout").req_flag(());
  let merged_dir = short('i')
    .long("input")
    .help("Merged directory (default: asset/merged)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/merged"));
  let common = common::parser(PathBuf::from("asset/merged_mlo"));
  construct!(command, merged_dir, common).map(|(_, merged_dir, common)| Deploy {
    merged_dir,
    common,
  })
}

/// Deploys only files whose contents need copying.
pub fn run(command: Deploy) -> Result<(), Box<dyn std::error::Error>> {
  let summary = crate::core::deploy::Deploy {
    merged_dir: command.merged_dir,
    source_cache_dir: command.common.source_cache,
    output_dir: command.common.output,
    force: command.common.force,
  }
  .run()?;
  println!(
    "Deployed {} files; {} unchanged; {} removed.",
    summary.copied, summary.skipped, summary.removed
  );
  Ok(())
}
