use std::path::PathBuf;

use bpaf::*;

use crate::core::{extract::ExtractYmap, merge::run::MergeYmapXml};

#[derive(Debug)]
pub enum Command {
  ParseYmapXml(ParseYmapXml),
  // ParseYmap(ParseYmap),
  MergeYmapXml(MergeYmapXml),
  ExtractYmap(ExtractYmap),
}

pub fn parse_args() -> Command {
  construct!([
    parse_ymap_xml(),
    // parse_ymap(),
    merge_ymap_xml(),
    extract_ymap(),
  ])
  .to_options()
  .run()
}

#[derive(Debug)]
pub struct ParseYmapXml {
  pub input: PathBuf,
}

fn parse_ymap_xml() -> impl Parser<Command> {
  let flag = long("parse-ymap-xml").help("Parse YMAP XML file").req_flag(());

  let input = short('i').long("input").argument::<PathBuf>("FILE");

  construct!(flag, input).map(|(_, input)| {
    Command::ParseYmapXml(ParseYmapXml {
      input,
    })
  })
}

fn merge_ymap_xml() -> impl Parser<Command> {
  let flag = long("merge-ymap-xml").help("Merge YMAP XML files").req_flag(());
  let vanilla_dir = long("vanilla-dir")
    .help("Directory containing vanilla YMAP XML files")
    .argument::<PathBuf>("DIR");
  let mod_dir =
    long("mod-dir").help("Directory containing mod YMAP XML files").argument::<PathBuf>("DIR");
  let output_dir =
    long("output-dir").help("Directory to save merged YMAP XML files").argument::<PathBuf>("DIR");
  let mod_ymap_dir = long("mod-ymap-dir")
    .help("Directory containing extracted mod YMAP files for reference during merging")
    .argument::<PathBuf>("DIR");
  let rebuild_all = long("rebuild-all")
    .help("Rebuild all YMAP files, even if only one mod reference exists")
    .switch();

  construct!(flag, vanilla_dir, mod_dir, output_dir, mod_ymap_dir, rebuild_all).map(
    |(_, vanilla_dir, mod_dir, output_dir, mod_ymap_dir, rebuild_all)| {
      Command::MergeYmapXml(MergeYmapXml {
        vanilla_dir,
        mod_dir,
        output_dir,
        mod_ymap_dir,
        rebuild_all,
      })
    },
  )
}

fn extract_ymap() -> impl Parser<Command> {
  let flag = long("extract-ymap").help("Extract YMAP files from MLO directory").req_flag(());
  let input = short('i').long("input").argument::<PathBuf>("DIR").help("MLO source directory");
  let output = short('o')
    .long("output")
    .argument::<PathBuf>("DIR")
    .help("Output directory for extracted YMAP files");
  let flatten = long("flatten")
    .help("Flatten output structure using resource_name___filename.ymap format")
    .switch();
  let vanilla_dir = long("vanilla-dir")
    .argument::<PathBuf>("DIR")
    .help("Only extract files that exist in vanilla directory")
    .optional();

  construct!(flag, input, output, flatten, vanilla_dir).map(
    |(_, input, output, flatten, vanilla_dir)| {
      Command::ExtractYmap(ExtractYmap {
        input_dir: input,
        output_dir: output,
        flatten,
        vanilla_dir,
      })
    },
  )
}
