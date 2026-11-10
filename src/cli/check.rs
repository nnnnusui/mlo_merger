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
  let overwrite_dir = directory(
    "overwrite",
    "Overwrites the overwrite cache is built from (default: asset/overwrite)",
    "asset/overwrite",
  );
  let overwrite_cache_dir = directory(
    "overwrite-cache",
    "Overwrite cache directory (default: asset/overwrite-cache)",
    "asset/overwrite-cache",
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
