use std::path::{Path, PathBuf};

use crate::cli::common::map_dir;
use bpaf::*;

use crate::app::map::Check;

/// Parses `--check`, which checks every store against its metadata without changing files.
pub fn parser(asset: &Path) -> impl Parser<Check> + use<> {
  options(asset).to_options().descr("Check every store for files missing from, or not recorded in, its metadata, and broken vanilla-cache links").command("--check")
}

fn options(asset: &Path) -> impl Parser<Check> + use<> {
  let map = map_dir(asset);
  let directory = |name: &'static str, help: &'static str, default: PathBuf| {
    long(name).help(help).argument::<PathBuf>("DIR").fallback(default)
  };
  let vanilla_dir = directory(
    "vanilla",
    "Vanilla archive directory (default: asset/vanilla)",
    asset.join("vanilla"),
  );
  let vanilla_cache_dir = directory(
    "vanilla-cache",
    "Derived vanilla cache directory (default: asset/map/vanilla-cache)",
    map.join("vanilla-cache"),
  );
  let source_cache_dir = directory(
    "source-cache",
    "Source cache directory (default: asset/map/source-cache)",
    map.join("source-cache"),
  );
  let overwrite_dir = directory(
    "overwrite",
    "Overwrites the overwrite cache is built from (default: asset/map/overwrite)",
    map.join("overwrite"),
  );
  let overwrite_cache_dir = directory(
    "overwrite-cache",
    "Overwrite cache directory (default: asset/map/overwrite-cache)",
    map.join("overwrite-cache"),
  );
  let merged_dir =
    directory("merged", "Merged directory (default: asset/map/merged)", map.join("merged"));
  let output_dir = short('o')
    .long("output")
    .help("Deployment directory (default: asset/map/merged_mlo)")
    .argument::<PathBuf>("DIR")
    .fallback(map.join("merged_mlo"));
  let deep = long("deep").help("Also compare the SHA-256 of every recorded file").switch();
  construct!(
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
