use super::common::{self, CommonOptions};
use bpaf::*;
use std::path::{Path, PathBuf};

/// Arguments for deploying generated merged and cached stream files.
#[derive(Debug, Clone)]
pub struct Deploy {
  /// Directory of generated merged files.
  pub merged_dir: PathBuf,
  /// Shared output, source-cache and force options.
  pub common: CommonOptions,
}

/// Parses `--deploy` arguments.
pub fn parser(asset: &Path) -> impl Parser<Deploy> + use<> {
  options(asset)
    .to_options()
    .descr("Copy merged and remaining cached files into a stream layout")
    .command("--deploy")
}

fn options(asset: &Path) -> impl Parser<Deploy> + use<> {
  let merged_dir = short('i')
    .long("input")
    .help("Merged directory (default: asset/merged)")
    .argument::<PathBuf>("DIR")
    .fallback(asset.join("merged"));
  let common = common::parser(asset, asset.join("merged_mlo"));
  construct!(merged_dir, common).map(|(merged_dir, common)| Deploy {
    merged_dir,
    common,
  })
}

/// Deploys only files whose contents need copying.
pub fn run(command: Deploy) -> Result<String, Box<dyn std::error::Error>> {
  let summary = crate::core::deploy::Deploy {
    merged_dir: command.merged_dir,
    source_cache_dir: command.common.source_cache,
    output_dir: command.common.output,
    force: command.common.force,
  }
  .run()?;
  Ok(format!(
    "Deployed {} files; {} unchanged; {} removed.\n",
    summary.copied, summary.skipped, summary.removed
  ))
}
