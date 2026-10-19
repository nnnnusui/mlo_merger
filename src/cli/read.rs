use std::path::PathBuf;

use bpaf::*;

use crate::core::read::{Read, ReadTarget};

/// Parses `--read [--resource NAME | --vanilla [VERSION]] [FILE...]`.
///
/// `--vanilla` takes an optional version, so `--vanilla map.ymap` reads `map.ymap` of the
/// latest version: a value naming a file is taken as the first file.
pub fn parser() -> impl Parser<Read> {
  let command = long("read")
    .help("Write stream file models as JSON under read-cache, skipping current ones")
    .req_flag(());
  let resource = long("resource")
    .help("Read from this resource (name or source-relative ID)")
    .argument::<String>("NAME")
    .map(Some);
  let vanilla_version = long("vanilla")
    .help("Read from this vanilla version, or the latest without one")
    .argument::<String>("VERSION")
    .map(|version| Some(Some(version)));
  let vanilla_latest = long("vanilla").req_flag(Some(None));
  let vanilla = construct!([vanilla_version, vanilla_latest]);
  let resource = resource.map(|resource| resource.map(ReadTarget::Resource));
  let vanilla = vanilla.map(|version| version.map(ReadTarget::Vanilla));
  let target = construct!([resource, vanilla]).fallback(None);
  let files = positional::<String>("FILE").many();
  let source_cache_dir = long("source-cache")
    .help("Source cache directory (default: asset/source-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/source-cache"));
  let vanilla_dir = long("vanilla-dir")
    .help("Vanilla archive directory (default: asset/vanilla)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/vanilla"));
  let output_dir = short('o')
    .long("output")
    .help("Output directory (default: asset/read-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/read-cache"));
  let force = short('f').long("force").help("Write every JSON again").switch();
  construct!(command, target, source_cache_dir, vanilla_dir, output_dir, force, files).map(
    |(_, target, source_cache_dir, vanilla_dir, output_dir, force, mut files)| {
      let target = match target {
        Some(ReadTarget::Vanilla(Some(version))) if names_file(&version) => {
          files.insert(0, version);
          ReadTarget::Vanilla(None)
        }
        target => target.unwrap_or(ReadTarget::Everywhere),
      };
      Read {
        target,
        files,
        source_cache_dir,
        vanilla_dir,
        output_dir,
        force,
      }
    },
  )
}

/// Whether a `--vanilla` value names a file rather than a version.
fn names_file(value: &str) -> bool {
  value.contains('.')
}

/// Writes the JSON and prints what was read as one line of JSON.
pub fn run(command: Read) -> Result<(), Box<dyn std::error::Error>> {
  let result = command.run()?;
  println!("{}", serde_json::to_string(&result)?);
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn parse(args: &[&str]) -> Read {
    parser().to_options().run_inner(args).unwrap()
  }

  #[test]
  fn targets_resources_vanilla_versions_or_everything() {
    let read = parse(&["--read", "--resource", "[maps]/alpha", "map.ymap"]);
    assert_eq!(read.target, ReadTarget::Resource("[maps]/alpha".into()));
    assert_eq!(read.files, ["map.ymap"]);
    assert_eq!(read.output_dir, PathBuf::from("asset/read-cache"));
    assert!(!read.force);
    assert!(parse(&["--read", "--resource", "alpha"]).files.is_empty());

    let read = parse(&["--read", "--vanilla", "map.ymap"]);
    assert_eq!(read.target, ReadTarget::Vanilla(None));
    assert_eq!(read.files, ["map.ymap"]);
    let read = parse(&["--read", "--vanilla", "0012-mpheist", "map.ymap", "-f"]);
    assert_eq!(read.target, ReadTarget::Vanilla(Some("0012-mpheist".into())));
    assert_eq!(read.files, ["map.ymap"]);
    assert!(read.force);
    assert_eq!(parse(&["--read", "--vanilla"]).target, ReadTarget::Vanilla(None));
    let read = parse(&["--read", "--vanilla", "mpheist"]);
    assert_eq!(read.target, ReadTarget::Vanilla(Some("mpheist".into())));
    assert!(read.files.is_empty());

    let read = parse(&["--read", "map.ymap", "-o", "out", "--vanilla-dir", "v"]);
    assert_eq!(read.target, ReadTarget::Everywhere);
    assert_eq!(read.output_dir, PathBuf::from("out"));
    assert_eq!(read.vanilla_dir, PathBuf::from("v"));

    let read = parse(&["--read", "a.ymap", "b/c.ymap", "-f"]);
    assert_eq!(read.target, ReadTarget::Everywhere);
    assert_eq!(read.files, ["a.ymap", "b/c.ymap"]);
    assert!(read.force);
    let read = parse(&["--read", "--vanilla", "a.ymap", "b.ymap"]);
    assert_eq!(read.target, ReadTarget::Vanilla(None));
    assert_eq!(read.files, ["a.ymap", "b.ymap"]);
    let read = parse(&["--read", "--resource", "alpha", "a.ymap", "b.ymap"]);
    assert_eq!(read.files, ["a.ymap", "b.ymap"]);

    assert!(parser().to_options().run_inner(&["--read", "--resource", "a", "--vanilla"]).is_err());
  }
}
