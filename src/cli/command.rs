use std::path::PathBuf;

use bpaf::*;

#[derive(Debug)]
pub enum Command {
  ParseYmapXml(ParseYmapXml),
  ParseYmap(ParseYmap),
}

pub fn parse_args() -> Command {
  construct!([
    parse_ymap_xml(),
    parse_ymap(),
  ])
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

  let input = short('i')
    .long("input")
    .argument::<PathBuf>("FILE");

  construct!(flag, input).map(|((), input)| Command::ParseYmapXml(ParseYmapXml { input }))
}

#[derive(Debug)]
pub struct ParseYmap {
  pub example: PathBuf,
}

fn parse_ymap() -> impl Parser<Command> {
  let flag = long("parse-ymap")
    .help("Parse YMAP file")
    .req_flag(());

  let example = short('e')
    .long("example")
    .argument::<PathBuf>("FILE");

  construct!(flag, example).map(|((), example)| Command::ParseYmap(ParseYmap { example }))
}
