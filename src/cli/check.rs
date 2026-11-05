use std::path::PathBuf;

use bpaf::*;

use crate::core::check::Check;

/// Parses `--check`, which checks every store against its metadata without changing files.
pub fn parser() -> impl Parser<Check> {
  let command = long("check")
    .help("Check every store for files missing from, or not recorded in, its metadata, and broken vanilla-cache links")
    .req_flag(());
  let directory = |name: &'static str, help: &'static str, default: &'static str| {
    long(name).help(help).argument::<PathBuf>("DIR").fallback(PathBuf::from(default))
  };
  let vanilla_dir =
    directory("vanilla", "Vanilla archive directory (default: asset/vanilla)", "asset/vanilla");
  let vanilla_cache_dir = directory(
    "vanilla-cache",
    "Derived vanilla cache directory (default: asset/vanilla-cache)",
    "asset/vanilla-cache",
  );
  let source_cache_dir = directory(
    "source-cache",
    "Source cache directory (default: asset/source-cache)",
    "asset/source-cache",
  );
  let merged_dir = directory("merged", "Merged directory (default: asset/merged)", "asset/merged");
  let output_dir = short('o')
    .long("output")
    .help("Deployment directory (default: asset/merged_mlo)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/merged_mlo"));
  let deep = long("deep").help("Also compare the SHA-256 of every recorded file").switch();
  construct!(
    command,
    vanilla_dir,
    vanilla_cache_dir,
    source_cache_dir,
    merged_dir,
    output_dir,
    deep
  )
  .map(|(_, vanilla_dir, vanilla_cache_dir, source_cache_dir, merged_dir, output_dir, deep)| {
    Check {
      vanilla_dir,
      vanilla_cache_dir,
      source_cache_dir,
      merged_dir,
      output_dir,
      deep,
    }
  })
}

/// Prints the issues as one line of JSON and fails when there are any.
pub fn run(command: Check) -> Result<(), Box<dyn std::error::Error>> {
  let report = command.run()?;
  println!("{}", serde_json::to_string(&report)?);
  match report.issues.len() {
    0 => Ok(()),
    count => Err(format!("{count} inconsistencies found").into()),
  }
}
