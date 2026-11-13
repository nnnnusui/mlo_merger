use std::path::PathBuf;

use bpaf::*;

use crate::core::read::{Read, ReadTarget};

/// A parsed `--read`, whose `--gamebuild` is resolved when it runs.
#[derive(Debug, Clone)]
pub struct ReadCommand {
  /// The read, without its game build.
  pub read: Read,
  /// `--gamebuild` as given.
  pub gamebuild: Option<String>,
}

/// Parses `--read [--resource NAME [--version SHA256] | --vanilla | --all] [--gamebuild BUILD]
/// [FILE...]`.
pub fn parser() -> impl Parser<ReadCommand> {
  let command = long("read")
    .help("Write stream file models as JSON under read-cache, skipping current ones")
    .req_flag(());
  let resource = long("resource")
    .help("Read from this resource (name or source-relative ID)")
    .argument::<String>("NAME")
    .map(ReadTarget::Resource);
  let vanilla = long("vanilla").help("Read from vanilla").req_flag(ReadTarget::Vanilla);
  let all = long("all").help("Read every resource and vanilla").req_flag(ReadTarget::All);
  let target = construct!([resource, vanilla, all]).fallback(ReadTarget::Everywhere);
  let gamebuild = long("gamebuild")
    .help("Read vanilla as of this game build: a build number or DLC name from [gamebuild] in the config file (default: the vanilla cache's)")
    .argument::<String>("BUILD")
    .optional();
  let files = positional::<String>("FILE").many();
  let source_dir = long("source")
    .help("Source resource directory, for resource files not moved into the source cache (default: asset/source)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/source"));
  let source_cache_dir = long("source-cache")
    .help("Source cache directory (default: asset/source-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/source-cache"));
  let vanilla_dir = long("vanilla-dir")
    .help("Vanilla archive directory (default: asset/vanilla)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/vanilla"));
  let vanilla_cache_dir = long("vanilla-cache")
    .help("Vanilla cache whose game build vanilla is read as of without --gamebuild (default: asset/vanilla-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/vanilla-cache"));
  let output_dir = short('o')
    .long("output")
    .help("Output directory (default: asset/read-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/read-cache"));
  let force = short('f').long("force").help("Write every JSON again").switch();
  let version = long("version")
    .help("With --resource, read the earlier version with this SHA-256 of each named file, from the source cache's history")
    .argument::<String>("SHA256")
    .optional();
  construct!(
    command,
    target,
    gamebuild,
    source_dir,
    source_cache_dir,
    vanilla_dir,
    vanilla_cache_dir,
    output_dir,
    force,
    version,
    files
  )
  .map(
    |(
      _,
      target,
      gamebuild,
      source_dir,
      source_cache_dir,
      vanilla_dir,
      vanilla_cache_dir,
      output_dir,
      force,
      version,
      files,
    )| ReadCommand {
      read: Read {
        target,
        files,
        source_dir,
        source_cache_dir,
        vanilla_dir,
        vanilla_cache_dir,
        gamebuild: None,
        output_dir,
        force,
        version,
      },
      gamebuild,
    },
  )
}

/// Writes the JSON and prints what was read as one line of JSON.
pub fn run(command: ReadCommand) -> Result<String, Box<dyn std::error::Error>> {
  let read = Read {
    gamebuild: super::common::gamebuild(command.gamebuild.as_deref())?,
    ..command.read
  };
  Ok(format!("{}\n", serde_json::to_string(&read.run()?)?))
}

#[cfg(test)]
mod tests {
  use super::*;

  fn parse(args: &[&str]) -> Read {
    parser().to_options().run_inner(args).unwrap().read
  }

  #[test]
  fn targets_resources_vanilla_or_everything() {
    let read = parse(&["--read", "--resource", "[maps]/alpha", "map.ymap"]);
    assert_eq!(read.target, ReadTarget::Resource("[maps]/alpha".into()));
    assert_eq!(read.files, ["map.ymap"]);
    assert_eq!(read.output_dir, PathBuf::from("asset/read-cache"));
    assert_eq!(read.vanilla_cache_dir, PathBuf::from("asset/vanilla-cache"));
    assert!(read.gamebuild.is_none());
    assert!(!read.force);
    assert!(parse(&["--read", "--resource", "alpha"]).files.is_empty());

    let read = parse(&["--read", "--vanilla", "map.ymap", "-f"]);
    assert_eq!(read.target, ReadTarget::Vanilla);
    assert_eq!(read.files, ["map.ymap"]);
    assert!(read.force);
    assert_eq!(parse(&["--read", "--vanilla"]).target, ReadTarget::Vanilla);
    let read = parse(&["--read", "--vanilla", "a.ymap", "b.ymap"]);
    assert_eq!(read.files, ["a.ymap", "b.ymap"]);

    let read = parse(&["--read", "map.ymap", "-o", "out", "--vanilla-dir", "v"]);
    assert_eq!(read.target, ReadTarget::Everywhere);
    assert_eq!(read.output_dir, PathBuf::from("out"));
    assert_eq!(read.vanilla_dir, PathBuf::from("v"));

    let read = parse(&["--read", "a.ymap", "b/c.ymap", "-f"]);
    assert_eq!(read.target, ReadTarget::Everywhere);
    assert_eq!(read.files, ["a.ymap", "b/c.ymap"]);
    assert!(read.force);
    let read = parse(&["--read", "--resource", "alpha", "a.ymap", "b.ymap"]);
    assert_eq!(read.files, ["a.ymap", "b.ymap"]);

    assert!(parser().to_options().run_inner(&["--read", "--resource", "a", "--vanilla"]).is_err());

    let read = parse(&["--read", "--resource", "alpha", "--version", "abc", "a.ymap"]);
    assert_eq!(read.version.as_deref(), Some("abc"));
    assert!(parse(&["--read", "a.ymap"]).version.is_none());

    let read = parse(&["--read", "--all", "-f"]);
    assert_eq!(read.target, ReadTarget::All);
    assert!(read.files.is_empty());
    assert!(read.force);
    assert!(parser().to_options().run_inner(&["--read", "--all", "--vanilla"]).is_err());
  }

  #[test]
  fn gamebuild_is_kept_as_given() {
    let command = parser()
      .to_options()
      .run_inner(&["--read", "--vanilla", "--gamebuild", "xm18", "--vanilla-cache", "c"])
      .unwrap();
    assert_eq!(command.gamebuild.as_deref(), Some("xm18"));
    assert_eq!(command.read.vanilla_cache_dir, PathBuf::from("c"));
    assert!(parse(&["--read", "a.ymap"]).gamebuild.is_none());
  }

  #[test]
  fn an_unknown_gamebuild_fails_the_run() {
    let command = parser().to_options().run_inner(&["--read", "--gamebuild", "nothing"]).unwrap();
    assert!(run(command).unwrap_err().to_string().contains("Unknown gamebuild: nothing"));
  }
}
