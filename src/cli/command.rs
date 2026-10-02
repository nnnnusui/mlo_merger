use std::path::PathBuf;

use bpaf::*;

use crate::core::{extract::ExtractYmap, getprop::GetProp, merge::run::MergeYmapXml};

#[derive(Debug, Clone)]
pub enum Command {
  ParseYmapXml(ParseYmapXml),
  // ParseYmap(ParseYmap),
  MergeYmapXml(MergeYmapXml),
  ExtractYmap(ExtractYmap),
  GetProp(GetProp),
  Pipeline(Pipeline),
}

pub fn parse_args() -> Command {
  construct!([
    parse_ymap_xml(),
    // parse_ymap(),
    merge_ymap_xml(),
    extract_ymap(),
    get_prop(),
    pipeline(),
  ])
  .to_options()
  .run()
}

#[derive(Debug, Clone)]
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
  let blacklist_config = long("blacklist-config")
    .help("Path to blacklist configuration file (TOML)")
    .argument::<PathBuf>("FILE")
    .optional();

  construct!(flag, vanilla_dir, mod_dir, output_dir, mod_ymap_dir, rebuild_all, blacklist_config)
    .map(|(_, vanilla_dir, mod_dir, output_dir, mod_ymap_dir, rebuild_all, blacklist_config)| {
      Command::MergeYmapXml(MergeYmapXml {
        vanilla_dir,
        mod_dir,
        output_dir,
        mod_ymap_dir,
        rebuild_all,
        blacklist_config,
      })
    })
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

fn get_prop() -> impl Parser<Command> {
  let flag =
    long("get-prop").help("List unique prop (archetype) names from YMAP XML files").req_flag(());
  let input = short('i')
    .long("input")
    .argument::<PathBuf>("PATH")
    .help("YMAP XML file or directory containing YMAP XML files");

  construct!(flag, input).map(|(_, input)| {
    Command::GetProp(GetProp {
      input,
    })
  })
}

/// Default command run with no flags: extract -> ymap2xml -> merge -> xml2ymap,
/// then (if `output_resource_dir` is set) deploy into a FiveM resource layout.
/// All paths except `source_dir`/`output_resource_dir` are derived from `workspace`.
/// Must stay last in `parse_args`' alternation since its flags are all optional.
#[derive(Debug, Clone)]
pub struct Pipeline {
  pub workspace: PathBuf,
  pub source_dir: PathBuf,
  pub output_resource_dir: Option<PathBuf>,
  pub vanilla_xml_dir: PathBuf,
  pub extracted_dir: PathBuf,
  pub extracted_xml_dir: PathBuf,
  pub merged_xml_dir: PathBuf,
  pub merged_dir: PathBuf,
  pub log_dir: PathBuf,
  pub blacklist_config: Option<PathBuf>,
  pub native_ymap_to_xml: bool,
}

fn pipeline() -> impl Parser<Command> {
  let workspace = long("workspace")
    .help("Root directory holding vanilla/extracted/merged/blacklist/log (default: asset)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset"));
  let source_dir = long("source-dir")
    .help("MLO source directory (default: <workspace>/source)")
    .argument::<PathBuf>("DIR")
    .optional();
  let output_resource_dir = long("output-resource-dir")
    .help(
      "FiveM resource directory to deploy into: overwrites stream/ymap/merged, \
       stream/ymap/clone and omit.txt",
    )
    .argument::<PathBuf>("DIR")
    .optional();
  let native_ymap_to_xml = long("native-ymap-to-xml")
    .help("Use the native Rust backend for ymap -> xml; CodeWalker.Core remains the default")
    .switch();

  construct!(workspace, source_dir, output_resource_dir, native_ymap_to_xml).map(
    |(workspace, source_dir, output_resource_dir, native_ymap_to_xml)| {
      let blacklist_config = workspace.join("blacklist.toml");
      Command::Pipeline(Pipeline {
        source_dir: source_dir.unwrap_or_else(|| workspace.join("source")),
        output_resource_dir,
        vanilla_xml_dir: workspace.join("vanilla/ymap.xml"),
        extracted_dir: workspace.join("extracted"),
        extracted_xml_dir: workspace.join("extracted.xml"),
        merged_xml_dir: workspace.join("merged.xml"),
        merged_dir: workspace.join("merged"),
        log_dir: workspace.join("log"),
        blacklist_config: blacklist_config.exists().then_some(blacklist_config),
        native_ymap_to_xml,
        workspace,
      })
    },
  )
}
