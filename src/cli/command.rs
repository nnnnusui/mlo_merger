use std::path::PathBuf;

use bpaf::*;

#[derive(Debug)]
pub enum Command {
  ParseYmapXml(ParseYmapXml),
  ParseYmap(ParseYmap),
  MergeYmapXml(MergeYmapXml),
}

pub fn parse_args() -> Command {
  construct!([parse_ymap_xml(), parse_ymap(), merge_ymap_xml(),])
    .to_options()
    .run()
}

#[derive(Debug)]
pub struct ParseYmapXml {
  pub input: PathBuf,
}

fn parse_ymap_xml() -> impl Parser<Command> {
  let flag = long("parse-ymap-xml")
    .help("Parse YMAP XML file")
    .req_flag(());

  let input = short('i').long("input").argument::<PathBuf>("FILE");

  construct!(flag, input).map(|(_, input)| Command::ParseYmapXml(ParseYmapXml { input }))
}

#[derive(Debug)]
pub struct ParseYmap {
  pub example: PathBuf,
}

fn parse_ymap() -> impl Parser<Command> {
  let flag = long("parse-ymap").help("Parse YMAP file").req_flag(());

  let example = short('e').long("example").argument::<PathBuf>("FILE");

  construct!(flag, example).map(|(_, example)| Command::ParseYmap(ParseYmap { example }))
}

#[derive(Debug)]
pub struct MergeYmapXml {
  pub vanilla_dir: PathBuf,
  pub mod_dir: PathBuf,
  pub output_dir: PathBuf,
}

fn merge_ymap_xml() -> impl Parser<Command> {
  let flag = long("merge-ymap-xml")
    .help("Merge YMAP XML files")
    .req_flag(());
  let vanilla_dir = long("vanilla-dir")
    .help("Directory containing vanilla YMAP XML files")
    .argument::<PathBuf>("DIR");
  let mod_dir = long("mod-dir")
    .help("Directory containing mod YMAP XML files")
    .argument::<PathBuf>("DIR");
  let output_dir = long("output-dir")
    .help("Directory to save merged YMAP XML files")
    .argument::<PathBuf>("DIR");

  construct!(flag, vanilla_dir, mod_dir, output_dir).map(|(_, vanilla_dir, mod_dir, output_dir)| {
    Command::MergeYmapXml(MergeYmapXml {
      vanilla_dir,
      mod_dir,
      output_dir,
    })
  })
}
