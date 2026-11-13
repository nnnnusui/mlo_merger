use std::path::{Path, PathBuf};

use bpaf::*;

use crate::core::check::Check;

/// Parses `--check`, which checks every store against its metadata without changing files.
pub fn parser(asset: &Path) -> impl Parser<Check> + use<> {
  let command = long("check")
    .help("Check every store for files missing from, or not recorded in, its metadata, and broken vanilla-cache links")
    .req_flag(());
  let directory = |name: &'static str, help: &'static str, default: &str| {
    long(name).help(help).argument::<PathBuf>("DIR").fallback(asset.join(default))
  };
  let vanilla_dir =
    directory("vanilla", "Vanilla archive directory (default: asset/vanilla)", "vanilla");
  let vanilla_cache_dir = directory(
    "vanilla-cache",
    "Derived vanilla cache directory (default: asset/vanilla-cache)",
    "vanilla-cache",
  );
  let source_cache_dir = directory(
    "source-cache",
    "Source cache directory (default: asset/source-cache)",
    "source-cache",
  );
  let overwrite_dir = directory(
    "overwrite",
    "Overwrites the overwrite cache is built from (default: asset/overwrite)",
    "overwrite",
  );
  let overwrite_cache_dir = directory(
    "overwrite-cache",
    "Overwrite cache directory (default: asset/overwrite-cache)",
    "overwrite-cache",
  );
  let merged_dir = directory("merged", "Merged directory (default: asset/merged)", "merged");
  let output_dir = short('o')
    .long("output")
    .help("Deployment directory (default: asset/merged_mlo)")
    .argument::<PathBuf>("DIR")
    .fallback(asset.join("merged_mlo"));
  let deep = long("deep").help("Also compare the SHA-256 of every recorded file").switch();
  construct!(
    command,
    vanilla_dir,
    vanilla_cache_dir,
    source_cache_dir,
    overwrite_dir,
    overwrite_cache_dir,
    merged_dir,
    output_dir,
    deep
  )
  .map(
    |(
      _,
      vanilla_dir,
      vanilla_cache_dir,
      source_cache_dir,
      overwrite_dir,
      overwrite_cache_dir,
      merged_dir,
      output_dir,
      deep,
    )| Check {
      vanilla_dir,
      vanilla_cache_dir,
      source_cache_dir,
      overwrite_dir: Some(overwrite_dir),
      overwrite_cache_dir,
      merged_dir,
      output_dir,
      deep,
    },
  )
}

/// Prints the issues as one line of JSON and fails when there are any.
pub fn run(command: Check) -> super::command::Output {
  let report = command.run().and_then(|report| {
    let line = serde_json::to_string(&report)?;
    Ok((format!("{line}\n"), report.issues.len()))
  });
  match report {
    Err(error) => Err(error).into(),
    Ok((stdout, 0)) => Ok(stdout).into(),
    Ok((stdout, count)) => super::command::Output {
      stdout,
      result: Err(format!("{count} inconsistencies found").into()),
    },
  }
}
