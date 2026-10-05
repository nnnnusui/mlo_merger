use std::path::PathBuf;

use bpaf::*;

use crate::core::{
  diff_cache::BuildDiffCache,
  extract::ExtractYmap,
  getprop::GetProp,
  gtav_cache::{BuildGtavCache, ListVanillaVersions},
  merge::run::MergeYmapXml,
  stream_conflicts::CheckStreamConflicts,
};

#[derive(Debug, Clone)]
pub enum Command {
  ParseYmapXml(ParseYmapXml),
  // ParseYmap(ParseYmap),
  MergeYmapXml(MergeYmapXml),
  BuildYmapCache(BuildYmapCache),
  BuildGtavCache(BuildGtavCache),
  BuildDiffCache(BuildDiffCache),
  ListVanillaVersions(ListVanillaVersions),
  ExtractYmap(ExtractYmap),
  GetProp(GetProp),
  CheckStreamConflicts(CheckStreamConflicts),
  ToXml(ConvertFiles),
  FromXml(ConvertFilesFromXml),
  MergeYbn(MergeYbn),
  Pipeline(Pipeline),
}

pub fn parse_args() -> Command {
  construct!([
    parse_ymap_xml(),
    // parse_ymap(),
    merge_ymap_xml(),
    build_ymap_cache(),
    build_gtav_cache(),
    build_diff_cache(),
    list_vanilla_versions(),
    extract_ymap(),
    get_prop(),
    check_stream_conflicts(),
    to_xml(),
    from_xml(),
    merge_ybn(),
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

#[derive(Debug, Clone)]
pub struct BuildYmapCache {
  pub vanilla_dir: PathBuf,
}

fn build_ymap_cache() -> impl Parser<Command> {
  let flag =
    long("build-ymap-cache").help("Build or update the vanilla YMAP parent cache").req_flag(());
  let vanilla_dir = long("vanilla-dir")
    .help("Directory containing vanilla YMAP XML files")
    .argument::<PathBuf>("DIR");
  construct!(flag, vanilla_dir).map(|(_, vanilla_dir)| {
    Command::BuildYmapCache(BuildYmapCache {
      vanilla_dir,
    })
  })
}

fn build_gtav_cache() -> impl Parser<Command> {
  let flag = long("generate-gtav-cache")
    .help("Build ordered vanilla YMAP caches from the installed GTA V Legacy RPF archives")
    .req_flag(());
  let game_dir =
    short('i').long("input").help("Installed GTA V Legacy directory").argument::<PathBuf>("DIR");
  let output_dir = short('o')
    .long("output")
    .help("Vanilla archive cache output directory")
    .argument::<PathBuf>("DIR");
  construct!(flag, game_dir, output_dir).map(|(_, game_dir, output_dir)| {
    Command::BuildGtavCache(BuildGtavCache {
      game_dir,
      output_dir,
    })
  })
}

fn build_diff_cache() -> impl Parser<Command> {
  let flag = long("generate-diff-cache")
    .help("Infer vanilla versions and cache MLO YMAP differences")
    .req_flag(());
  let input_dir = short('i')
    .long("input")
    .help("Resource directory or resources root")
    .argument::<PathBuf>("DIR");
  let output_dir = short('o')
    .long("output")
    .help("Empty output directory for diff caches")
    .argument::<PathBuf>("DIR");
  let gtav_cache_dir = long("gtav-cache")
    .help("GTAV cache root (default: asset/gtav-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/gtav-cache"));
  construct!(flag, input_dir, output_dir, gtav_cache_dir).map(
    |(_, input_dir, output_dir, gtav_cache_dir)| {
      Command::BuildDiffCache(BuildDiffCache {
        input_dir,
        output_dir,
        gtav_cache_dir,
      })
    },
  )
}

#[cfg(test)]
mod gtav_tests {
  use super::*;

  #[test]
  fn vanilla_version_list_arguments_allow_default_or_explicit_cache() {
    let Command::ListVanillaVersions(command) = list_vanilla_versions()
      .to_options()
      .run_inner(&["--list-vanilla-versions", "example.ymap"])
      .unwrap()
    else {
      panic!("Expected version listing command")
    };
    assert_eq!(command.file_name, "example.ymap");
    assert_eq!(command.gtav_cache_dir, PathBuf::from("asset/gtav-cache"));
    let Command::ListVanillaVersions(command) = list_vanilla_versions()
      .to_options()
      .run_inner(&["--list-vanilla-versions", "example.ybn", "--gtav-cache", "custom"])
      .unwrap()
    else {
      panic!("Expected version listing command")
    };
    assert_eq!(command.gtav_cache_dir, PathBuf::from("custom"));
    assert_eq!(command.file_name, "example.ybn");
    assert!(list_vanilla_versions().to_options().run_inner(&["--list-vanilla-versions"]).is_err());
  }

  #[test]
  fn diff_cache_arguments_allow_default_or_explicit_gtav_cache() {
    let Command::BuildDiffCache(command) = build_diff_cache()
      .to_options()
      .run_inner(&["--generate-diff-cache", "-i", "mlo", "-o", "output"])
      .unwrap()
    else {
      panic!("Expected diff cache command")
    };
    assert_eq!(command.gtav_cache_dir, PathBuf::from("asset/gtav-cache"));
    let Command::BuildDiffCache(command) = build_diff_cache()
      .to_options()
      .run_inner(&["--generate-diff-cache", "--gtav-cache", "custom", "-i", "mlo", "-o", "output"])
      .unwrap()
    else {
      panic!("Expected diff cache command")
    };
    assert_eq!(command.gtav_cache_dir, PathBuf::from("custom"));
    assert!(
      build_diff_cache().to_options().run_inner(&["--generate-diff-cache", "-i", "mlo"]).is_err()
    );
  }

  #[test]
  fn gtav_selects_archive_cache_without_pipeline_flags() {
    let command = build_gtav_cache()
      .to_options()
      .run_inner(&["--generate-gtav-cache", "-i", "/mnt/gtav", "-o", "asset/gtav-cache"])
      .unwrap();
    let Command::BuildGtavCache(command) = command else {
      panic!("Expected archive cache command")
    };
    assert_eq!(command.game_dir, PathBuf::from("/mnt/gtav"));
    assert_eq!(command.output_dir, PathBuf::from("asset/gtav-cache"));
    let command = build_gtav_cache()
      .to_options()
      .run_inner(&["--generate-gtav-cache", "--input", "game", "--output", "cache"])
      .unwrap();
    let Command::BuildGtavCache(command) = command else {
      panic!("Expected archive cache command")
    };
    assert_eq!(command.output_dir, PathBuf::from("cache"));
    assert!(
      build_gtav_cache().to_options().run_inner(&["--generate-gtav-cache", "-i", "game"]).is_err()
    );
    assert!(
      build_gtav_cache().to_options().run_inner(&["--generate-gtav-cache", "-o", "cache"]).is_err()
    );
    assert!(build_gtav_cache().to_options().run_inner(&["-i", "game", "-o", "cache"]).is_err());
    assert!(build_gtav_cache().to_options().run_inner(&["--gtav", "game"]).is_err());
  }
}

fn list_vanilla_versions() -> impl Parser<Command> {
  let file_name = long("list-vanilla-versions")
    .help("List vanilla stages that added or modified a cached filename")
    .argument::<String>("FILE");
  let gtav_cache_dir = long("gtav-cache")
    .help("GTAV cache root (default: asset/gtav-cache)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset/gtav-cache"));
  construct!(file_name, gtav_cache_dir).map(|(file_name, gtav_cache_dir)| {
    Command::ListVanillaVersions(ListVanillaVersions {
      file_name,
      gtav_cache_dir,
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

fn check_stream_conflicts() -> impl Parser<Command> {
  let flag = long("check-stream-conflicts")
    .help("Find duplicate filenames across FiveM stream directories and write a JSON report")
    .req_flag(());
  let input = short('i')
    .long("input")
    .argument::<PathBuf>("DIR")
    .help("Resource root containing stream directories, or a stream directory");
  let output = short('o')
    .long("output")
    .argument::<PathBuf>("FILE")
    .help("Path to write the JSON conflict report");

  construct!(flag, input, output).map(|(_, input_dir, output_file)| {
    Command::CheckStreamConflicts(CheckStreamConflicts {
      input_dir,
      output_file,
    })
  })
}

#[derive(Debug, Clone)]
pub struct ConvertFiles {
  pub input: PathBuf,
  pub output_dir: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ConvertFilesFromXml {
  pub input: PathBuf,
  pub output_dir: PathBuf,
  pub schema_dir: Option<PathBuf>,
}

fn to_xml() -> impl Parser<Command> {
  let flag = long("to-xml").help("Convert supported native game files to XML").req_flag(());
  let input = short('i').long("input").argument::<PathBuf>("FILE_OR_DIR");
  let output_dir = short('o').long("output").argument::<PathBuf>("DIR").optional();
  construct!(flag, input, output_dir).map(|(_, input, output_dir)| {
    Command::ToXml(ConvertFiles {
      input,
      output_dir,
    })
  })
}

fn from_xml() -> impl Parser<Command> {
  let flag =
    long("from-xml").help("Convert supported XML game files to native binaries").req_flag(());
  let input = short('i').long("input").argument::<PathBuf>("FILE_OR_DIR");
  let output_dir = short('o').long("output").argument::<PathBuf>("DIR");
  let schema_dir = long("schema-dir")
    .help("META schema directory; required for .pso.xml, otherwise matching binaries are auto-discovered")
    .argument::<PathBuf>("DIR");
  let schema_dir = schema_dir.optional();
  construct!(flag, input, output_dir, schema_dir).map(|(_, input, output_dir, schema_dir)| {
    Command::FromXml(ConvertFilesFromXml {
      input,
      output_dir,
      schema_dir,
    })
  })
}

#[derive(Debug, Clone)]
pub struct MergeYbn {
  pub source_dir: PathBuf,
  pub vanilla_dir: PathBuf,
  pub output_dir: PathBuf,
  pub omitted_files_path: PathBuf,
  pub use_codewalker_dll: bool,
}

fn merge_ybn() -> impl Parser<Command> {
  let flag = long("merge-ybn")
    .help("Merge conflicting YBN resources against vanilla without running the full pipeline")
    .req_flag(());
  let workspace = long("workspace")
    .help("Root directory holding source, vanilla, and merged_ybn (default: asset)")
    .argument::<PathBuf>("DIR")
    .fallback(PathBuf::from("asset"));
  let source_dir = long("source-dir")
    .help("MLO source directory (default: <workspace>/source)")
    .argument::<PathBuf>("DIR")
    .optional();
  let use_codewalker_dll =
    long("use-codewalker-dll").help("Use CodeWalker.Core.dll to rebuild merged YBN files").switch();

  construct!(flag, workspace, source_dir, use_codewalker_dll).map(
    |(_, workspace, source_dir, use_codewalker_dll)| {
      let source_dir = source_dir.unwrap_or_else(|| workspace.join("source"));
      Command::MergeYbn(MergeYbn {
        source_dir,
        vanilla_dir: workspace.join("vanilla/ybn"),
        output_dir: workspace.join("merged_ybn"),
        omitted_files_path: workspace.join("extracted/_extracted_ybns.txt"),
        use_codewalker_dll,
      })
    },
  )
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
  pub use_codewalker_dll: bool,
  pub yes: bool,
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
       stream/ymap/clone, stream/ybn/merged and omit.txt",
    )
    .argument::<PathBuf>("DIR")
    .optional();
  let use_codewalker_dll = long("use-codewalker-dll")
    .help("Use CodeWalker.Core.dll for YMAP conversion and YBN BVH rebuilding; Native YMAP conversion is the default")
    .switch();
  let yes =
    short('y').long("yes").help("Skip confirmation before replacing generated outputs").switch();

  construct!(workspace, source_dir, output_resource_dir, use_codewalker_dll, yes).map(
    |(workspace, source_dir, output_resource_dir, use_codewalker_dll, yes)| {
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
        use_codewalker_dll,
        yes,
        workspace,
      })
    },
  )
}
